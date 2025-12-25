use std::{str::FromStr, sync::Arc};

use sqlx::{
    Error, Executor, Pool, Sqlite,
    sqlite::{
        SqliteConnectOptions, SqliteConnection, SqliteJournalMode, SqlitePoolOptions,
        SqliteSynchronous,
    },
};
use tracing::warn;
use utils::assets::asset_dir;

pub mod backup;
pub mod models;
pub mod validation;

pub use backup::{BackupInfo, BackupService};

/// Apply performance pragmas to a SQLite connection.
/// These pragmas are applied on every new connection via `after_connect`.
///
/// Pragmas applied:
/// - `temp_store = MEMORY` (2): Store temporary tables in memory
/// - `mmap_size = 256MB`: Memory-mapped I/O for faster reads
/// - `cache_size = -64000`: 64MB page cache (negative = KB)
async fn apply_performance_pragmas(conn: &mut SqliteConnection) -> Result<(), Error> {
    // temp_store = MEMORY (2)
    conn.execute("PRAGMA temp_store = 2").await?;

    // mmap_size = 256MB
    conn.execute("PRAGMA mmap_size = 268435456").await?;

    // cache_size = -64000 (64MB, negative means KB)
    conn.execute("PRAGMA cache_size = -64000").await?;

    Ok(())
}

#[derive(Clone)]
pub struct DBService {
    pub pool: Pool<Sqlite>,
}

impl DBService {
    pub async fn new() -> Result<DBService, Error> {
        let db_path = asset_dir().join("db.sqlite");
        let database_url = format!("sqlite://{}", db_path.to_string_lossy());
        let options = SqliteConnectOptions::from_str(&database_url)?
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal)
            .busy_timeout(std::time::Duration::from_secs(30));

        let pool = SqlitePoolOptions::new()
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
        Ok(DBService { pool })
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
        Ok(DBService { pool })
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
        let db_path = asset_dir().join("db.sqlite");
        let database_url = format!("sqlite://{}", db_path.to_string_lossy());
        let options = SqliteConnectOptions::from_str(&database_url)?
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal)
            .busy_timeout(std::time::Duration::from_secs(30));

        let pool = if let Some(hook) = after_connect {
            SqlitePoolOptions::new()
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
