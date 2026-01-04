use std::{path::Path, str::FromStr, sync::Arc, time::Duration};

use sqlx::{
    Error, Executor, Pool, Sqlite,
    sqlite::{
        SqliteConnectOptions, SqliteConnection, SqliteJournalMode, SqlitePoolOptions,
        SqliteSynchronous,
    },
};
use tracing::{error, info, warn};
use utils::assets::database_path;

pub mod backup;
pub mod backup_scheduler;
pub mod metrics;
pub mod models;
pub mod retry;
#[cfg(any(test, feature = "test-utils"))]
pub mod test_utils;
pub mod validation;
pub mod wal_monitor;

pub use backup::{BackupError, BackupInfo, BackupService};
pub use backup_scheduler::{BackupScheduler, BackupSchedulerConfig, BackupSchedulerHandle};
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

// ============================================================================
// Migration Recovery
// ============================================================================

/// Check if the broken migration 20260102051142 caused data loss and recover if possible.
///
/// Detection criteria:
/// - Migration 20260102051142 was applied (in _sqlx_migrations)
/// - task_attempts table is empty (data was deleted by CASCADE)
/// - tasks table has data (should have had attempts)
/// - A backup with task_attempts data exists
///
/// Recovery:
/// - Restore from the most recent backup that has task_attempts data
/// - Migrations will re-run safely on the restored database
///
/// This is a sync function because it must run BEFORE the SQLx pool is created.
fn check_and_recover_from_migration_data_loss(db_path: &Path) -> Result<(), std::io::Error> {
    use rusqlite::Connection;

    // Check if database even exists
    if !db_path.exists() {
        return Ok(());
    }

    // Open connection to check state
    let conn = match Connection::open(db_path) {
        Ok(c) => c,
        Err(e) => {
            warn!(error = ?e, "Failed to open database for migration recovery check");
            return Ok(());
        }
    };

    // Check if broken migration was applied
    let migration_applied: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM _sqlx_migrations WHERE version = 20260102051142)",
            [],
            |row| row.get(0),
        )
        .unwrap_or(false);

    if !migration_applied {
        return Ok(()); // Migration hasn't run yet, no recovery needed
    }

    // Check if task_attempts is empty (data loss indicator)
    let attempts_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM task_attempts", [], |row| row.get(0))
        .unwrap_or(0);

    if attempts_count > 0 {
        return Ok(()); // Data exists, no recovery needed
    }

    // Check if tasks exist that should have attempts
    let tasks_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM tasks", [], |row| row.get(0))
        .unwrap_or(0);

    if tasks_count == 0 {
        return Ok(()); // No tasks, no recovery needed
    }

    drop(conn); // Close connection before restore

    // Find backup with attempts data
    let Some(backup_path) = BackupService::find_backup_with_attempts(db_path) else {
        warn!(
            "Data loss detected from migration 20260102051142 but no backup with attempts found. \
             Task attempt history cannot be recovered."
        );
        return Ok(());
    };

    info!(
        backup = %backup_path.display(),
        tasks = tasks_count,
        "Detected data loss from migration 20260102051142, restoring from backup"
    );

    // Read backup data
    let backup_data = std::fs::read(&backup_path)?;

    // Restore from backup (this replaces the current db)
    BackupService::restore_from_data(db_path, &backup_data)?;

    info!(
        "Database restored from backup successfully. \
         Migrations will re-run safely with PRAGMA foreign_keys = OFF."
    );

    Ok(())
}

// ============================================================================
// Database Integrity Check
// ============================================================================

/// Check database integrity using PRAGMA quick_check.
///
/// This is faster than full integrity_check and catches most corruption issues.
/// Returns Ok(()) if database is healthy, Err with details if corrupted.
///
/// This is a sync function because it must run BEFORE the SQLx pool is created.
fn check_database_integrity(db_path: &Path) -> Result<(), String> {
    use rusqlite::Connection;

    if !db_path.exists() {
        return Ok(()); // No database to check
    }

    let conn = Connection::open(db_path)
        .map_err(|e| format!("Failed to open database for integrity check: {}", e))?;

    // Run quick_check first (faster, catches most issues)
    let result: String = conn
        .query_row("PRAGMA quick_check", [], |row| row.get(0))
        .map_err(|e| format!("Failed to run integrity check: {}", e))?;

    if result != "ok" {
        return Err(format!("Database integrity check failed: {}", result));
    }

    Ok(())
}

/// Attempt to recover from database corruption by restoring from the most recent backup.
///
/// Returns Ok(true) if recovery was successful, Ok(false) if no backup available,
/// or Err if recovery failed.
fn attempt_corruption_recovery(db_path: &Path) -> Result<bool, std::io::Error> {
    // Find most recent backup
    let backup_dir = utils::assets::backup_dir();
    if !backup_dir.exists() {
        return Ok(false);
    }

    // Get all backups sorted by modification time (newest first)
    let mut backups: Vec<_> = std::fs::read_dir(&backup_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| {
            let path = e.path();
            path.extension().is_some_and(|ext| ext == "sqlite")
                && path
                    .file_name()
                    .is_some_and(|n| n.to_string_lossy().starts_with("db_backup_"))
        })
        .collect();

    if backups.is_empty() {
        return Ok(false);
    }

    backups.sort_by(|a, b| {
        let a_time = a.metadata().and_then(|m| m.modified()).ok();
        let b_time = b.metadata().and_then(|m| m.modified()).ok();
        b_time.cmp(&a_time)
    });

    let backup_path = backups[0].path();

    warn!(
        backup = %backup_path.display(),
        "Attempting automatic recovery from backup"
    );

    // Read backup data
    let backup_data = std::fs::read(&backup_path)?;

    // Restore from backup
    BackupService::restore_from_data(db_path, &backup_data)?;

    // Verify restored database is healthy
    match check_database_integrity(db_path) {
        Ok(()) => {
            info!(
                backup = %backup_path.display(),
                "Database restored and verified healthy"
            );
            Ok(true)
        }
        Err(msg) => {
            error!(
                backup = %backup_path.display(),
                error = %msg,
                "Restored database is also corrupted"
            );
            Err(std::io::Error::other(format!(
                "Restored backup is also corrupted: {}",
                msg
            )))
        }
    }
}

#[derive(Clone)]
pub struct DBService {
    pub pool: Pool<Sqlite>,
    pub metrics: DbMetrics,
}

impl DBService {
    /// Create a minimal DBService with just a connection pool.
    /// Use this for bootstrapping (e.g., creating hooks) before full initialization.
    /// Does NOT run migrations, backups, or integrity checks.
    pub async fn bootstrap() -> Result<DBService, Error> {
        let db_path = database_path();
        let database_url = format!("sqlite://{}", db_path.to_string_lossy());

        let options = SqliteConnectOptions::from_str(&database_url)?
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal)
            .busy_timeout(Duration::from_secs(DEFAULT_ACQUIRE_TIMEOUT_SECS));

        let pool = SqlitePoolOptions::new()
            .max_connections(2) // Minimal connections for bootstrap
            .min_connections(1)
            .acquire_timeout(Duration::from_secs(DEFAULT_ACQUIRE_TIMEOUT_SECS))
            .after_connect(|conn, _meta| {
                Box::pin(async move { apply_performance_pragmas(conn).await })
            })
            .connect_with(options)
            .await?;

        let metrics = DbMetrics::new();
        Ok(DBService { pool, metrics })
    }

    pub async fn new() -> Result<DBService, Error> {
        let db_path = database_path();

        // Execute pending restore FIRST (before any connections)
        match BackupService::execute_pending_restore() {
            Ok(true) => info!("Pending database restore executed successfully"),
            Ok(false) => {} // No pending restore
            Err(e) => warn!(error = ?e, "Failed to execute pending restore"),
        }

        // Check database integrity BEFORE creating pool
        match check_database_integrity(&db_path) {
            Ok(()) => {
                info!("Database integrity check passed");
            }
            Err(msg) => {
                error!(error = %msg, "DATABASE CORRUPTION DETECTED");

                // Attempt automatic recovery from backup
                match attempt_corruption_recovery(&db_path) {
                    Ok(true) => {
                        info!("Automatic recovery from backup successful");
                    }
                    Ok(false) => {
                        error!("No backup available for recovery. Database is corrupted.");
                        return Err(Error::Protocol(msg));
                    }
                    Err(e) => {
                        error!(error = ?e, "Automatic recovery failed");
                        return Err(Error::Protocol(format!(
                            "Database corruption detected and recovery failed: {}",
                            msg
                        )));
                    }
                }
            }
        }

        // Check for data loss from broken migration and auto-recover BEFORE pool creation
        if let Err(e) = check_and_recover_from_migration_data_loss(&db_path) {
            warn!(error = ?e, "Migration recovery check failed");
        }

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

        // Check if there are pending migrations before deciding to backup
        let has_pending = has_pending_migrations(&pool).await;

        // Only create pre-migration backup if there are migrations to run
        if has_pending {
            info!("Pending migrations detected, creating pre-migration backup");
            let db_path_for_backup = db_path.clone();
            let backup_result = tokio::task::spawn_blocking(move || {
                BackupService::backup_before_migration(&db_path_for_backup)
            })
            .await;

            if let Err(e) = backup_result {
                warn!(error = ?e, "Backup task panicked");
            } else if let Err(e) = backup_result.unwrap() {
                warn!(error = ?e, "Failed to create pre-migration backup");
            }

            // Run migrations with foreign keys disabled to prevent CASCADE deletes during table recreation.
            // IMPORTANT: PRAGMA foreign_keys cannot be changed inside a transaction, and SQLx wraps
            // migrations in transactions. We use a dedicated connection with FK off for migrations.
            run_migrations_with_fk_disabled(&database_url).await?;
        }

        // Always cleanup old backups (this is fast and doesn't block)
        let db_path_for_cleanup = db_path.clone();
        tokio::spawn(async move {
            if let Err(e) = tokio::task::spawn_blocking(move || {
                BackupService::cleanup_old_backups(&db_path_for_cleanup)
            })
            .await
            {
                warn!(error = ?e, "Failed to cleanup old backups");
            }
        });

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

        // Execute pending restore FIRST (before any connections)
        match BackupService::execute_pending_restore() {
            Ok(true) => info!("Pending database restore executed successfully"),
            Ok(false) => {} // No pending restore
            Err(e) => warn!(error = ?e, "Failed to execute pending restore"),
        }

        // Check database integrity BEFORE creating pool
        match check_database_integrity(&db_path) {
            Ok(()) => {
                info!("Database integrity check passed");
            }
            Err(msg) => {
                error!(error = %msg, "DATABASE CORRUPTION DETECTED");

                // Attempt automatic recovery from backup
                match attempt_corruption_recovery(&db_path) {
                    Ok(true) => {
                        info!("Automatic recovery from backup successful");
                    }
                    Ok(false) => {
                        error!("No backup available for recovery. Database is corrupted.");
                        return Err(Error::Protocol(msg));
                    }
                    Err(e) => {
                        error!(error = ?e, "Automatic recovery failed");
                        return Err(Error::Protocol(format!(
                            "Database corruption detected and recovery failed: {}",
                            msg
                        )));
                    }
                }
            }
        }

        // Check for data loss from broken migration and auto-recover BEFORE pool creation
        if let Err(e) = check_and_recover_from_migration_data_loss(&db_path) {
            warn!(error = ?e, "Migration recovery check failed");
        }

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

        // Check if there are pending migrations before deciding to backup
        let has_pending = has_pending_migrations(&pool).await;

        // Only create pre-migration backup if there are migrations to run
        if has_pending {
            info!("Pending migrations detected, creating pre-migration backup");
            let db_path_for_backup = db_path.clone();
            let backup_result = tokio::task::spawn_blocking(move || {
                BackupService::backup_before_migration(&db_path_for_backup)
            })
            .await;

            if let Err(e) = backup_result {
                warn!(error = ?e, "Backup task panicked");
            } else if let Err(e) = backup_result.unwrap() {
                warn!(error = ?e, "Failed to create pre-migration backup");
            }

            // Run migrations with foreign keys disabled to prevent CASCADE deletes during table recreation.
            run_migrations_with_fk_disabled(&database_url).await?;
        }

        // Always cleanup old backups in background (this is fast)
        let db_path_for_cleanup = db_path.clone();
        tokio::spawn(async move {
            if let Err(e) = tokio::task::spawn_blocking(move || {
                BackupService::cleanup_old_backups(&db_path_for_cleanup)
            })
            .await
            {
                warn!(error = ?e, "Failed to cleanup old backups");
            }
        });

        Ok(pool)
    }
}

/// Check if there are pending migrations to run.
///
/// Compares the migrations in the codebase against the `_sqlx_migrations` table
/// to determine if any migrations need to be applied.
async fn has_pending_migrations(pool: &Pool<Sqlite>) -> bool {
    let migrator = sqlx::migrate!("./migrations");
    let applied: Vec<i64> = match sqlx::query_scalar::<_, i64>(
        "SELECT version FROM _sqlx_migrations ORDER BY version",
    )
    .fetch_all(pool)
    .await
    {
        Ok(versions) => versions,
        Err(_) => {
            // Table doesn't exist or query failed - assume we need migrations
            return true;
        }
    };

    // Check if any migration in the migrator is not in the applied list
    for migration in migrator.iter() {
        if !applied.contains(&migration.version) {
            return true;
        }
    }

    false
}

/// Run migrations with foreign keys disabled.
///
/// SQLite's PRAGMA foreign_keys cannot be changed inside a transaction, and SQLx wraps
/// migrations in transactions. To prevent CASCADE deletes during table recreation migrations,
/// we must disable foreign keys at the connection level BEFORE running migrations.
///
/// This function:
/// 1. Creates a single-connection pool with FK disabled via after_connect
/// 2. Runs all pending migrations
/// 3. Pool is dropped after migrations complete
async fn run_migrations_with_fk_disabled(database_url: &str) -> Result<(), Error> {
    let options = SqliteConnectOptions::from_str(database_url)?
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .busy_timeout(Duration::from_secs(DEFAULT_ACQUIRE_TIMEOUT_SECS));

    // Create a single-connection pool with FK disabled for migrations
    let migration_pool = SqlitePoolOptions::new()
        .max_connections(1)
        .min_connections(1)
        .after_connect(|conn, _meta| {
            Box::pin(async move {
                // Disable foreign keys for this connection
                conn.execute("PRAGMA foreign_keys = OFF").await?;
                Ok(())
            })
        })
        .connect_with(options)
        .await?;

    // Run migrations with FK disabled
    sqlx::migrate!("./migrations").run(&migration_pool).await?;

    // Pool is dropped here, connection closed
    Ok(())
}
