//! Integration tests for SQLite performance pragmas.
//!
//! These tests verify that performance pragmas are correctly applied to connections:
//! - journal_mode = WAL
//! - synchronous = NORMAL
//! - temp_store = MEMORY
//! - mmap_size = 256MB
//! - cache_size = -64000 (64MB)

use std::str::FromStr;

use sqlx::{
    Executor, Row, SqlitePool,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
};
use tempfile::TempDir;

/// Apply performance pragmas to a connection.
/// This is the function that should be used with `after_connect`.
async fn apply_performance_pragmas(
    conn: &mut sqlx::sqlite::SqliteConnection,
) -> Result<(), sqlx::Error> {
    // temp_store = MEMORY (2)
    sqlx::query("PRAGMA temp_store = 2")
        .execute(&mut *conn)
        .await?;

    // mmap_size = 256MB
    sqlx::query("PRAGMA mmap_size = 268435456")
        .execute(&mut *conn)
        .await?;

    // cache_size = -64000 (64MB, negative means KB)
    sqlx::query("PRAGMA cache_size = -64000")
        .execute(&mut *conn)
        .await?;

    Ok(())
}

/// Create a SQLite pool with performance pragmas applied.
async fn setup_pool_with_pragmas() -> (SqlitePool, TempDir) {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");

    let options =
        SqliteConnectOptions::from_str(&format!("sqlite://{}", db_path.to_string_lossy()))
            .expect("Invalid database URL")
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal);

    let pool = SqlitePoolOptions::new()
        .after_connect(|conn, _meta| Box::pin(async move { apply_performance_pragmas(conn).await }))
        .connect_with(options)
        .await
        .expect("Failed to create pool");

    // Run migrations
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("Failed to run migrations");

    (pool, temp_dir)
}

#[tokio::test]
async fn test_sqlite_pragma_journal_mode_wal() {
    let (pool, _temp_dir) = setup_pool_with_pragmas().await;

    let row = pool
        .fetch_one(sqlx::query("PRAGMA journal_mode"))
        .await
        .expect("Failed to query journal_mode");

    let journal_mode: String = row.get(0);
    assert_eq!(
        journal_mode.to_lowercase(),
        "wal",
        "Journal mode should be WAL"
    );
}

#[tokio::test]
async fn test_sqlite_pragma_synchronous_normal() {
    let (pool, _temp_dir) = setup_pool_with_pragmas().await;

    let row = pool
        .fetch_one(sqlx::query("PRAGMA synchronous"))
        .await
        .expect("Failed to query synchronous");

    let synchronous: i32 = row.get(0);
    // NORMAL = 1
    assert_eq!(synchronous, 1, "Synchronous should be NORMAL (1)");
}

#[tokio::test]
async fn test_sqlite_pragma_temp_store_memory() {
    let (pool, _temp_dir) = setup_pool_with_pragmas().await;

    let row = pool
        .fetch_one(sqlx::query("PRAGMA temp_store"))
        .await
        .expect("Failed to query temp_store");

    let temp_store: i32 = row.get(0);
    // MEMORY = 2
    assert_eq!(temp_store, 2, "temp_store should be MEMORY (2)");
}

#[tokio::test]
async fn test_sqlite_pragma_mmap_size() {
    let (pool, _temp_dir) = setup_pool_with_pragmas().await;

    let row = pool
        .fetch_one(sqlx::query("PRAGMA mmap_size"))
        .await
        .expect("Failed to query mmap_size");

    let mmap_size: i64 = row.get(0);
    // 256MB = 268435456 bytes
    assert_eq!(mmap_size, 268_435_456, "mmap_size should be 256MB");
}

#[tokio::test]
async fn test_sqlite_pragma_cache_size() {
    let (pool, _temp_dir) = setup_pool_with_pragmas().await;

    let row = pool
        .fetch_one(sqlx::query("PRAGMA cache_size"))
        .await
        .expect("Failed to query cache_size");

    let cache_size: i32 = row.get(0);
    // -64000 means 64MB (negative means KB)
    assert_eq!(cache_size, -64000, "cache_size should be -64000 (64MB)");
}

#[tokio::test]
async fn test_sqlite_pragmas_applied_to_all_connections() {
    let (pool, _temp_dir) = setup_pool_with_pragmas().await;

    // Acquire multiple connections and verify pragmas on each
    for i in 0..3 {
        let mut conn = pool.acquire().await.expect("Failed to acquire connection");

        let row = sqlx::query("PRAGMA temp_store")
            .fetch_one(&mut *conn)
            .await
            .expect("Failed to query temp_store");

        let temp_store: i32 = row.get(0);
        assert_eq!(
            temp_store, 2,
            "Connection {} should have temp_store = MEMORY",
            i
        );
    }
}
