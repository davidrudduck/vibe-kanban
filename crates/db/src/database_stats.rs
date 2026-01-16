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
    /// Total number of tasks in the database
    pub task_count: i64,
    /// Total number of task attempts in the database
    pub task_attempt_count: i64,
    /// Total number of execution processes in the database
    pub execution_process_count: i64,
    /// Total number of log entries in the database
    pub log_entry_count: i64,
}

/// Result of a VACUUM operation.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct VacuumResult {
    /// Database size before VACUUM in bytes
    pub size_before_bytes: i64,
    /// Database size after VACUUM in bytes
    pub size_after_bytes: i64,
    /// Bytes freed by the VACUUM operation
    pub freed_bytes: i64,
}

/// Retrieve database statistics including file sizes, page info, and table counts.
///
/// # Arguments
/// * `pool` - SQLite connection pool
/// * `db_path` - Path to the main database file
///
/// # Returns
/// `DatabaseStats` with all statistics populated
pub async fn get_database_stats(
    pool: &SqlitePool,
    db_path: &Path,
) -> Result<DatabaseStats, DatabaseStatsError> {
    // Get file sizes from filesystem
    let database_size_bytes = if db_path.exists() {
        std::fs::metadata(db_path)?.len() as i64
    } else {
        return Err(DatabaseStatsError::NotFound);
    };

    let wal_path = db_path.with_extension("sqlite-wal");
    let wal_size_bytes = if wal_path.exists() {
        std::fs::metadata(&wal_path)?.len() as i64
    } else {
        0
    };

    // Get page information from PRAGMA
    let page_size: i64 = sqlx::query_scalar("SELECT page_size FROM pragma_page_size()")
        .fetch_one(pool)
        .await?;

    let freelist_count: i64 =
        sqlx::query_scalar("SELECT freelist_count FROM pragma_freelist_count()")
            .fetch_one(pool)
            .await?;

    // Use freelist_count as free_pages (pages that are free and can be reclaimed)
    let free_pages = freelist_count;

    // Get table counts
    let task_count: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) as "count: i64" FROM tasks"#)
        .fetch_one(pool)
        .await?;

    let task_attempt_count: i64 =
        sqlx::query_scalar(r#"SELECT COUNT(*) as "count: i64" FROM task_attempts"#)
            .fetch_one(pool)
            .await?;

    let execution_process_count: i64 =
        sqlx::query_scalar(r#"SELECT COUNT(*) as "count: i64" FROM execution_processes"#)
            .fetch_one(pool)
            .await?;

    let log_entry_count: i64 =
        sqlx::query_scalar(r#"SELECT COUNT(*) as "count: i64" FROM log_entries"#)
            .fetch_one(pool)
            .await?;

    Ok(DatabaseStats {
        database_size_bytes,
        wal_size_bytes,
        free_pages,
        page_size,
        task_count,
        task_attempt_count,
        execution_process_count,
        log_entry_count,
    })
}

/// Run VACUUM on the database to reclaim space from deleted records.
///
/// VACUUM rebuilds the database file, packing it into a minimal amount of disk space.
/// This operation can take a while on large databases and requires exclusive access.
///
/// # Arguments
/// * `pool` - SQLite connection pool
/// * `db_path` - Path to the main database file
///
/// # Returns
/// `VacuumResult` with before/after sizes and bytes freed
pub async fn vacuum_database(
    pool: &SqlitePool,
    db_path: &Path,
) -> Result<VacuumResult, DatabaseStatsError> {
    if !db_path.exists() {
        return Err(DatabaseStatsError::NotFound);
    }

    let size_before_bytes = std::fs::metadata(db_path)?.len() as i64;

    // Run VACUUM
    sqlx::query("VACUUM").execute(pool).await?;

    let size_after_bytes = std::fs::metadata(db_path)?.len() as i64;

    Ok(VacuumResult {
        size_before_bytes,
        size_after_bytes,
        freed_bytes: size_before_bytes - size_after_bytes,
    })
}

/// Run ANALYZE on the database to update query planner statistics.
///
/// ANALYZE gathers statistics about the contents of tables that the query planner
/// can use to make better choices about how to execute queries.
///
/// # Arguments
/// * `pool` - SQLite connection pool
pub async fn analyze_database(pool: &SqlitePool) -> Result<(), DatabaseStatsError> {
    sqlx::query("ANALYZE").execute(pool).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode};
    use std::str::FromStr;
    use tempfile::TempDir;

    /// Create a test SQLite pool with migrations applied.
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
        let (pool, temp_dir) = setup_test_pool().await;
        let db_path = temp_dir.path().join("test.db");

        let stats = get_database_stats(&pool, &db_path).await.unwrap();

        // Verify stats have valid values
        assert!(
            stats.database_size_bytes > 0,
            "Database size should be positive"
        );
        assert!(stats.page_size > 0, "Page size should be positive");
        assert!(stats.task_count >= 0, "Task count should be non-negative");
        assert!(
            stats.task_attempt_count >= 0,
            "Task attempt count should be non-negative"
        );
        assert!(
            stats.execution_process_count >= 0,
            "Execution process count should be non-negative"
        );
        assert!(
            stats.log_entry_count >= 0,
            "Log entry count should be non-negative"
        );
    }

    #[tokio::test]
    async fn test_vacuum_database() {
        let (pool, temp_dir) = setup_test_pool().await;
        let db_path = temp_dir.path().join("test.db");

        let result = vacuum_database(&pool, &db_path).await.unwrap();

        // VACUUM should execute without error
        assert!(
            result.size_before_bytes > 0,
            "Size before should be positive"
        );
        assert!(result.size_after_bytes > 0, "Size after should be positive");
        // freed_bytes can be 0 or positive (negative is unlikely but technically possible
        // if concurrent writes happen)
    }

    #[tokio::test]
    async fn test_analyze_database() {
        let (pool, _temp_dir) = setup_test_pool().await;

        // ANALYZE should execute without error
        let result = analyze_database(&pool).await;
        assert!(result.is_ok(), "ANALYZE should succeed");
    }

    #[tokio::test]
    async fn test_get_database_stats_not_found() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let db_path = temp_dir.path().join("nonexistent.db");

        // Create a pool pointing to a different (existing) database
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

        // Try to get stats for a non-existent path
        let result = get_database_stats(&pool, &db_path).await;
        assert!(
            matches!(result, Err(DatabaseStatsError::NotFound)),
            "Should return NotFound error"
        );
    }
}
