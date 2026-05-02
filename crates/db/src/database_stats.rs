//! Database statistics and maintenance operations.
//!
//! Provides functions to retrieve database statistics (file sizes, table counts, page info)
//! and perform maintenance operations like VACUUM and ANALYZE.

use std::path::Path;

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use thiserror::Error;
use ts_rs::TS;

/// Error type for database stats operations.
#[derive(Debug, Error)]
pub enum DatabaseStatsError {
    #[error("Database file not found")]
    NotFound,
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
}

/// Statistics about the SQLite database.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DatabaseStats {
    /// Size of the main database file in bytes
    pub database_size_bytes: i64,
    /// Size of the WAL (Write-Ahead Log) file in bytes
    pub wal_size_bytes: i64,
    /// Number of free pages in the database (reclaimable with VACUUM)
    pub free_pages: i64,
    /// Size of each database page in bytes
    pub page_size: i64,
    /// Total number of pages in the database
    pub page_count: i64,
    /// Total number of tasks in the database
    pub task_count: i64,
    /// Total number of workspaces in the database
    pub workspace_count: i64,
    /// Total number of execution processes in the database
    pub execution_process_count: i64,
    /// Total number of legacy log rows (expected near-zero post-migration)
    pub legacy_log_row_count: i64,
}

/// Result of a VACUUM operation.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct VacuumResult {
    /// Bytes freed by the VACUUM operation
    pub bytes_freed: i64,
}

/// Result of an ANALYZE operation.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AnalyzeResult {
    /// Whether the ANALYZE operation succeeded
    pub success: bool,
}

/// Retrieve database statistics including file sizes, page info, and table counts.
pub async fn get_database_stats(
    pool: &SqlitePool,
    db_path: &Path,
) -> Result<DatabaseStats, DatabaseStatsError> {
    let database_size_bytes = if db_path.exists() {
        std::fs::metadata(db_path)?.len() as i64
    } else {
        return Err(DatabaseStatsError::NotFound);
    };

    // Construct WAL path by appending "-wal" to the full db path string.
    // Using string concatenation avoids Path::with_extension stripping ".sqlite" from "db.v2.sqlite".
    let wal_path_str = db_path.to_string_lossy().to_string() + "-wal";
    let wal_path = std::path::PathBuf::from(&wal_path_str);
    let wal_size_bytes = if wal_path.exists() {
        std::fs::metadata(&wal_path)?.len() as i64
    } else {
        0
    };

    // Use a single acquired connection for all PRAGMA queries to ensure consistency.
    let mut conn = pool.acquire().await?;

    let page_size: i64 = sqlx::query_scalar("SELECT page_size FROM pragma_page_size()")
        .fetch_one(&mut *conn)
        .await?;

    let freelist_count: i64 =
        sqlx::query_scalar("SELECT freelist_count FROM pragma_freelist_count()")
            .fetch_one(&mut *conn)
            .await?;

    let page_count: i64 = sqlx::query_scalar("PRAGMA page_count")
        .fetch_one(&mut *conn)
        .await?;

    let task_count: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) as "count: i64" FROM tasks"#)
        .fetch_one(&mut *conn)
        .await?;

    let workspace_count: i64 =
        sqlx::query_scalar(r#"SELECT COUNT(*) as "count: i64" FROM workspaces"#)
            .fetch_one(&mut *conn)
            .await?;

    let execution_process_count: i64 =
        sqlx::query_scalar(r#"SELECT COUNT(*) as "count: i64" FROM execution_processes"#)
            .fetch_one(&mut *conn)
            .await?;

    let legacy_log_row_count: i64 =
        sqlx::query_scalar(r#"SELECT COUNT(*) as "count: i64" FROM execution_process_logs"#)
            .fetch_one(&mut *conn)
            .await?;

    Ok(DatabaseStats {
        database_size_bytes,
        wal_size_bytes,
        free_pages: freelist_count,
        page_size,
        page_count,
        task_count,
        workspace_count,
        execution_process_count,
        legacy_log_row_count,
    })
}

/// Runs SQLite VACUUM to reclaim free pages and defragment the database.
///
/// # Concurrency
/// VACUUM requires exclusive access to the database. The connection pool is
/// configured with a 5-second `busy_timeout`, so SQLite will automatically
/// retry for up to 5 seconds if other connections are active. Under sustained
/// heavy load the operation may still return `SQLITE_BUSY` — callers should
/// handle this error and surface it to the user for manual retry.
pub async fn vacuum_database(pool: &SqlitePool) -> Result<VacuumResult, DatabaseStatsError> {
    let mut conn = pool.acquire().await?;

    let page_size: i64 = sqlx::query_scalar("SELECT page_size FROM pragma_page_size()")
        .fetch_one(&mut *conn)
        .await?;

    let before_pages: i64 = sqlx::query_scalar("SELECT page_count FROM pragma_page_count()")
        .fetch_one(&mut *conn)
        .await?;

    sqlx::query("VACUUM").execute(&mut *conn).await?;

    let after_pages: i64 = sqlx::query_scalar("SELECT page_count FROM pragma_page_count()")
        .fetch_one(&mut *conn)
        .await?;

    Ok(VacuumResult {
        bytes_freed: (before_pages - after_pages) * page_size,
    })
}

/// Run ANALYZE on the database to update query planner statistics.
pub async fn analyze_database(pool: &SqlitePool) -> Result<AnalyzeResult, DatabaseStatsError> {
    sqlx::query("ANALYZE").execute(pool).await?;
    Ok(AnalyzeResult { success: true })
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode};
    use tempfile::TempDir;

    use super::*;

    async fn setup_test_pool() -> (SqlitePool, TempDir) {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let db_path = temp_dir.path().join("test.db");

        let options =
            SqliteConnectOptions::from_str(&format!("sqlite://{}", db_path.to_string_lossy()))
                .expect("Invalid database URL")
                .create_if_missing(true)
                .journal_mode(SqliteJournalMode::Wal);

        let pool = SqlitePool::connect_with(options)
            .await
            .expect("Failed to create pool");

        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .expect("Failed to run migrations");

        (pool, temp_dir)
    }

    #[tokio::test]
    async fn test_get_database_stats() {
        let (pool, temp_dir): (SqlitePool, TempDir) = setup_test_pool().await;
        let db_path = temp_dir.path().join("test.db");

        let stats = get_database_stats(&pool, &db_path).await.unwrap();

        assert!(
            stats.database_size_bytes > 0,
            "Database size should be positive"
        );
        assert!(stats.page_size > 0, "Page size should be positive");
        assert!(stats.page_count > 0, "Page count should be positive");
        assert_eq!(stats.task_count, 0, "Task count should be zero on empty DB");
        assert_eq!(
            stats.workspace_count, 0,
            "Workspace count should be zero on empty DB"
        );
        assert!(
            stats.execution_process_count >= 0,
            "Execution process count should be non-negative"
        );
        assert!(
            stats.legacy_log_row_count >= 0,
            "Legacy log row count should be non-negative"
        );
    }

    #[tokio::test]
    async fn test_vacuum_database() {
        let (pool, _temp_dir) = setup_test_pool().await;

        let result = vacuum_database(&pool).await.unwrap();
        assert!(
            result.bytes_freed >= 0,
            "Bytes freed should be non-negative"
        );
    }

    #[tokio::test]
    async fn test_analyze_database() {
        let (pool, _temp_dir) = setup_test_pool().await;

        let result = analyze_database(&pool).await;
        assert!(result.is_ok(), "ANALYZE should succeed");
        assert!(
            result.unwrap().success,
            "AnalyzeResult.success should be true"
        );
    }

    #[tokio::test]
    async fn test_get_database_stats_not_found() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let db_path = temp_dir.path().join("nonexistent.db");

        let actual_db_path = temp_dir.path().join("test.db");
        let options = SqliteConnectOptions::from_str(&format!(
            "sqlite://{}",
            actual_db_path.to_string_lossy()
        ))
        .expect("Invalid database URL")
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal);

        let pool = SqlitePool::connect_with(options)
            .await
            .expect("Failed to create pool");

        let result = get_database_stats(&pool, &db_path).await;
        assert!(
            matches!(result, Err(DatabaseStatsError::NotFound)),
            "Should return NotFound error"
        );
    }
}
