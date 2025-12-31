use std::{str::FromStr, sync::Arc, time::Duration};

use sqlx::{
    Error, Executor, Pool, Sqlite,
    sqlite::{
        SqliteConnectOptions, SqliteConnection, SqliteJournalMode, SqlitePoolOptions,
        SqliteSynchronous,
    },
};
use tracing::warn;
use utils::assets::database_path;

pub mod backup;
pub mod metrics;
pub mod models;
pub mod retry;
pub mod validation;
pub mod wal_monitor;

pub use backup::{BackupInfo, BackupService};
pub use metrics::DbMetrics;
pub use retry::{RetryConfig, is_retryable_error, with_retry};
pub use wal_monitor::{WalMonitor, WalMonitorConfig, WalMonitorHandle, get_wal_size};

// ============================================================================
// Connection Pool Configuration
// ============================================================================

/// Default maximum connections in the pool.
/// SQLite benefits from limited connections due to single-writer model.
const DEFAULT_MAX_CONNECTIONS: u32 = 10;

/// Minimum idle connections to maintain.
const DEFAULT_MIN_CONNECTIONS: u32 = 2;

/// Connection acquisition timeout in seconds.
const DEFAULT_ACQUIRE_TIMEOUT_SECS: u64 = 30;

/// Idle connection timeout in seconds (10 minutes).
const DEFAULT_IDLE_TIMEOUT_SECS: u64 = 600;

/// Get max connections from environment or use default.
fn get_max_connections() -> u32 {
    std::env::var("VK_SQLITE_MAX_CONNECTIONS")
        .ok()
        .and_then(|s| s.parse::<u32>().ok())
        .filter(|&n| n > 0 && n <= 100)
        .unwrap_or(DEFAULT_MAX_CONNECTIONS)
}

/// Apply performance and reliability pragmas to a SQLite connection.
/// These pragmas are applied on every new connection via `after_connect`.
///
/// Performance pragmas:
/// - `temp_store = MEMORY` (2): Store temporary tables in memory
/// - `mmap_size`: Memory-mapped I/O for faster reads (64MB dev, 256MB prod)
/// - `cache_size = -64000`: 64MB page cache (negative = KB)
/// - `synchronous = NORMAL`: Must be set AFTER mmap_size to ensure proper fsync
///
/// WAL tuning:
/// - `wal_autocheckpoint = 2000`: Checkpoint every ~8MB instead of default 4MB
///   This reduces checkpoint frequency under heavy write load
///
/// CRITICAL: The `synchronous` pragma must be set AFTER `mmap_size` because
/// enabling mmap can affect how SQLite handles fsync. Without explicit
/// synchronous setting after mmap, disk I/O errors (code 522) can occur
/// under heavy write load.
async fn apply_performance_pragmas(conn: &mut SqliteConnection) -> Result<(), Error> {
    // temp_store = MEMORY (2)
    conn.execute("PRAGMA temp_store = 2").await?;

    // mmap_size: Use smaller value for dev to reduce I/O pressure
    // Debug builds (dev): 64MB - sufficient for typical dev database (<100MB)
    // Release builds (prod): 256MB - better performance for larger databases
    #[cfg(debug_assertions)]
    conn.execute("PRAGMA mmap_size = 67108864").await?; // 64MB

    #[cfg(not(debug_assertions))]
    conn.execute("PRAGMA mmap_size = 268435456").await?; // 256MB

    // CRITICAL: Set synchronous AFTER mmap_size to ensure disk writes are
    // properly synchronized. Without this, mmap'ed writes can bypass fsync
    // guarantees and cause SQLITE_IOERR (code 522) under load.
    conn.execute("PRAGMA synchronous = NORMAL").await?;

    // cache_size = -64000 (64MB, negative means KB)
    conn.execute("PRAGMA cache_size = -64000").await?;

    // WAL checkpoint tuning: checkpoint every 2000 pages (~8MB)
    // Default is 1000 pages (~4MB). Larger threshold reduces checkpoint frequency
    // which helps under heavy write load.
    conn.execute("PRAGMA wal_autocheckpoint = 2000").await?;

    Ok(())
}

#[derive(Clone)]
pub struct DBService {
    pub pool: Pool<Sqlite>,
    pub metrics: DbMetrics,
}

impl DBService {
    pub async fn new() -> Result<DBService, Error> {
        let db_path = database_path();
        let database_url = format!("sqlite://{}", db_path.to_string_lossy());
        let max_connections = get_max_connections();

        tracing::info!(
            max_connections = max_connections,
            min_connections = DEFAULT_MIN_CONNECTIONS,
            "Initializing SQLite connection pool"
        );

        let options = SqliteConnectOptions::from_str(&database_url)?
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal)
            .busy_timeout(Duration::from_secs(DEFAULT_ACQUIRE_TIMEOUT_SECS));

        let pool = SqlitePoolOptions::new()
            .max_connections(max_connections)
            .min_connections(DEFAULT_MIN_CONNECTIONS)
            .acquire_timeout(Duration::from_secs(DEFAULT_ACQUIRE_TIMEOUT_SECS))
            .idle_timeout(Some(Duration::from_secs(DEFAULT_IDLE_TIMEOUT_SECS)))
            .after_connect(|conn, _meta| {
                Box::pin(async move { apply_performance_pragmas(conn).await })
            })
            .connect_with(options)
            .await?;

        // Create pre-migration backup for safety
        if let Err(e) = BackupService::backup_before_migration(&db_path) {
            warn!(error = ?e, "Failed to create pre-migration backup");
        }

        // Clean up old backups (keep last 5)
        if let Err(e) = BackupService::cleanup_old_backups(&db_path) {
            warn!(error = ?e, "Failed to cleanup old backups");
        }

        sqlx::migrate!("./migrations").run(&pool).await?;

        let metrics = DbMetrics::new();
        Ok(DBService { pool, metrics })
    }

    pub async fn new_with_after_connect<F>(after_connect: F) -> Result<DBService, Error>
    where
        F: for<'a> Fn(
                &'a mut SqliteConnection,
            ) -> std::pin::Pin<
                Box<dyn std::future::Future<Output = Result<(), Error>> + Send + 'a>,
            > + Send
            + Sync
            + 'static,
    {
        let pool = Self::create_pool(Some(Arc::new(after_connect))).await?;
        let metrics = DbMetrics::new();
        Ok(DBService { pool, metrics })
    }

    async fn create_pool<F>(after_connect: Option<Arc<F>>) -> Result<Pool<Sqlite>, Error>
    where
        F: for<'a> Fn(
                &'a mut SqliteConnection,
            ) -> std::pin::Pin<
                Box<dyn std::future::Future<Output = Result<(), Error>> + Send + 'a>,
            > + Send
            + Sync
            + 'static,
    {
        let db_path = database_path();
        let database_url = format!("sqlite://{}", db_path.to_string_lossy());
        let max_connections = get_max_connections();

        tracing::info!(
            max_connections = max_connections,
            min_connections = DEFAULT_MIN_CONNECTIONS,
            "Initializing SQLite connection pool"
        );

        let options = SqliteConnectOptions::from_str(&database_url)?
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal)
            .busy_timeout(Duration::from_secs(DEFAULT_ACQUIRE_TIMEOUT_SECS));

        let pool = if let Some(hook) = after_connect {
            SqlitePoolOptions::new()
                .max_connections(max_connections)
                .min_connections(DEFAULT_MIN_CONNECTIONS)
                .acquire_timeout(Duration::from_secs(DEFAULT_ACQUIRE_TIMEOUT_SECS))
                .idle_timeout(Some(Duration::from_secs(DEFAULT_IDLE_TIMEOUT_SECS)))
                .after_connect(move |conn, _meta| {
                    let hook = hook.clone();
                    Box::pin(async move {
                        // Apply performance pragmas first
                        apply_performance_pragmas(conn).await?;
                        // Then run user-provided hook
                        hook(conn).await?;
                        Ok(())
                    })
                })
                .connect_with(options)
                .await?
        } else {
            SqlitePoolOptions::new()
                .max_connections(max_connections)
                .min_connections(DEFAULT_MIN_CONNECTIONS)
                .acquire_timeout(Duration::from_secs(DEFAULT_ACQUIRE_TIMEOUT_SECS))
                .idle_timeout(Some(Duration::from_secs(DEFAULT_IDLE_TIMEOUT_SECS)))
                .after_connect(|conn, _meta| {
                    Box::pin(async move { apply_performance_pragmas(conn).await })
                })
                .connect_with(options)
                .await?
        };

        // Create pre-migration backup for safety
        if let Err(e) = BackupService::backup_before_migration(&db_path) {
            warn!(error = ?e, "Failed to create pre-migration backup");
        }

        // Clean up old backups (keep last 5)
        if let Err(e) = BackupService::cleanup_old_backups(&db_path) {
            warn!(error = ?e, "Failed to cleanup old backups");
        }

        sqlx::migrate!("./migrations").run(&pool).await?;
        Ok(pool)
    }
}
