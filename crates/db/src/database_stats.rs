//! Database statistics and maintenance operations.
//!
//! Provides functions to retrieve database statistics (file sizes, table counts, page info)
//! and perform maintenance operations like VACUUM and ANALYZE.

use std::path::Path;

use serde::{Deserialize, Serialize};
use sqlx::{Pool, Sqlite};
use ts_rs::TS;

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
    /// Total number of workspaces in the database
    pub workspace_count: i64,
    /// Total number of execution processes in the database
    pub execution_process_count: i64,
    /// Number of rows in execution_process_logs (legacy table, expected near-zero post-migration)
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
    pub success: bool,
}

/// Retrieve database statistics including file sizes, page info, and table counts.
pub async fn get_database_stats(
    pool: &Pool<Sqlite>,
    db_path: &Path,
) -> Result<DatabaseStats, sqlx::Error> {
    // Get WAL size from filesystem
    let database_size_bytes = std::fs::metadata(db_path)
        .map(|m| m.len() as i64)
        .unwrap_or(0);

    let wal_path = db_path.with_extension("sqlite-wal");
    let wal_size_bytes = std::fs::metadata(&wal_path)
        .map(|m| m.len() as i64)
        .unwrap_or(0);

    // Use a dedicated connection for PRAGMA queries to avoid pool contention
    let mut conn = pool.acquire().await?;

    let page_count: i64 = sqlx::query_scalar("PRAGMA page_count")
        .fetch_one(&mut *conn)
        .await?;

    let page_size: i64 = sqlx::query_scalar("PRAGMA page_size")
        .fetch_one(&mut *conn)
        .await?;

    let freelist_count: i64 = sqlx::query_scalar("PRAGMA freelist_count")
        .fetch_one(&mut *conn)
        .await?;

    let free_pages = freelist_count;
    let _ = page_count; // available if needed for future use

    // Table row counts
    let task_count: i64 =
        sqlx::query_scalar(r#"SELECT COUNT(*) FROM tasks"#)
            .fetch_one(&mut *conn)
            .await?;

    let workspace_count: i64 =
        sqlx::query_scalar(r#"SELECT COUNT(*) FROM workspaces"#)
            .fetch_one(&mut *conn)
            .await?;

    let execution_process_count: i64 =
        sqlx::query_scalar(r#"SELECT COUNT(*) FROM execution_processes"#)
            .fetch_one(&mut *conn)
            .await?;

    let legacy_log_row_count: i64 =
        sqlx::query_scalar(r#"SELECT COUNT(*) FROM execution_process_logs"#)
            .fetch_one(&mut *conn)
            .await?;

    Ok(DatabaseStats {
        database_size_bytes,
        wal_size_bytes,
        free_pages,
        page_size,
        task_count,
        workspace_count,
        execution_process_count,
        legacy_log_row_count,
    })
}

/// Run VACUUM on the database to reclaim space from deleted records.
///
/// VACUUM rebuilds the database file, packing it into a minimal amount of disk space.
/// This operation cannot run in a transaction and requires a dedicated connection.
pub async fn vacuum_database(pool: &Pool<Sqlite>) -> Result<VacuumResult, sqlx::Error> {
    let mut conn = pool.acquire().await?;

    let page_count_before: i64 = sqlx::query_scalar("PRAGMA page_count")
        .fetch_one(&mut *conn)
        .await?;

    let page_size: i64 = sqlx::query_scalar("PRAGMA page_size")
        .fetch_one(&mut *conn)
        .await?;

    sqlx::query("VACUUM").execute(&mut *conn).await?;

    let page_count_after: i64 = sqlx::query_scalar("PRAGMA page_count")
        .fetch_one(&mut *conn)
        .await?;

    let bytes_freed = (page_count_before - page_count_after) * page_size;

    Ok(VacuumResult { bytes_freed })
}

/// Run ANALYZE on the database to update query planner statistics.
pub async fn analyze_database(pool: &Pool<Sqlite>) -> Result<AnalyzeResult, sqlx::Error> {
    let mut conn = pool.acquire().await?;
    sqlx::query("ANALYZE").execute(&mut *conn).await?;
    Ok(AnalyzeResult { success: true })
}
