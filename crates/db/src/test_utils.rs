//! Test utilities for database tests.
//!
//! This module provides helper functions for creating test database pools.
//! It centralizes the pool creation logic to ensure consistent configuration
//! across all tests.

use sqlx::{
    SqlitePool,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions},
};
use std::str::FromStr;
use std::sync::OnceLock;
use std::time::Duration;
use tempfile::TempDir;
use tokio::sync::OnceCell;

/// Shared template database for faster test setup.
/// The template has migrations already applied.
static TEMPLATE_DIR: OnceLock<TempDir> = OnceLock::new();
static TEMPLATE_READY: OnceCell<()> = OnceCell::const_new();

/// Get or create the template database directory.
/// This creates a database with migrations applied that can be copied for tests.
fn get_template_dir() -> &'static TempDir {
    TEMPLATE_DIR.get_or_init(|| TempDir::new().expect("Failed to create template temp dir"))
}

/// Ensure the template database is ready (migrations applied).
async fn ensure_template_ready() {
    TEMPLATE_READY
        .get_or_init(|| async {
            let template_path = get_template_dir().path().join("template.db");

            let options =
                SqliteConnectOptions::from_str(&format!("sqlite://{}", template_path.display()))
                    .expect("Invalid template database URL")
                    .create_if_missing(true)
                    .journal_mode(SqliteJournalMode::Wal);

            let pool = SqlitePoolOptions::new()
                .min_connections(0)
                .max_connections(1)
                .connect_with(options)
                .await
                .expect("Failed to create template pool");

            // Run migrations on template
            sqlx::migrate!("./migrations")
                .run(&pool)
                .await
                .expect("Failed to run migrations on template");

            // Close the pool to release the file
            pool.close().await;

            tracing::debug!("Template database ready at {:?}", template_path);
        })
        .await;
}

/// Create a test database pool with migrations applied.
///
/// This uses a template database approach for speed:
/// 1. First test creates a template database with migrations
/// 2. Subsequent tests copy the template file (much faster than re-running migrations)
///
/// Returns the pool and a TempDir that must be kept alive for the duration of the test.
pub async fn create_test_pool() -> (SqlitePool, TempDir) {
    // Ensure template is ready
    ensure_template_ready().await;

    // Create a temp dir for this test
    let temp_dir = TempDir::new().expect("Failed to create test temp dir");
    let db_path = temp_dir.path().join("test.db");

    // Copy template database to test location
    let template_path = get_template_dir().path().join("template.db");
    std::fs::copy(&template_path, &db_path).expect("Failed to copy template database");

    // Also copy WAL and SHM files if they exist (they shouldn't after pool.close())
    let wal_path = template_path.with_extension("db-wal");
    let shm_path = template_path.with_extension("db-shm");
    if wal_path.exists() {
        let _ = std::fs::copy(&wal_path, db_path.with_extension("db-wal"));
    }
    if shm_path.exists() {
        let _ = std::fs::copy(&shm_path, db_path.with_extension("db-shm"));
    }

    // Create pool for the copied database
    let options = SqliteConnectOptions::from_str(&format!("sqlite://{}", db_path.display()))
        .expect("Invalid test database URL")
        .journal_mode(SqliteJournalMode::Wal);

    let pool = SqlitePoolOptions::new()
        .min_connections(1)
        .max_connections(5)
        .acquire_timeout(Duration::from_secs(5))
        .connect_with(options)
        .await
        .expect("Failed to create test pool");

    (pool, temp_dir)
}

/// Create a test pool the old way (running migrations each time).
/// Use this when you need to test migration behavior.
pub async fn create_test_pool_with_migrations() -> (SqlitePool, TempDir) {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");

    let options = SqliteConnectOptions::from_str(&format!("sqlite://{}", db_path.display()))
        .expect("Invalid database URL")
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal);

    let pool = SqlitePoolOptions::new()
        .min_connections(1)
        .max_connections(5)
        .acquire_timeout(Duration::from_secs(5))
        .connect_with(options)
        .await
        .expect("Failed to create pool");

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("Failed to run migrations");

    (pool, temp_dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_test_pool() {
        let (pool, _temp_dir) = create_test_pool().await;

        // Verify the pool works and has tables
        let result: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM projects")
            .fetch_one(&pool)
            .await
            .expect("Failed to query projects table");

        assert_eq!(result.0, 0); // Empty table
    }

    #[tokio::test]
    async fn test_template_reuse() {
        // Create two pools to verify template reuse works
        let (pool1, _temp1) = create_test_pool().await;
        let (pool2, _temp2) = create_test_pool().await;

        // Both should work
        let _: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM projects")
            .fetch_one(&pool1)
            .await
            .expect("Pool 1 should work");

        let _: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM projects")
            .fetch_one(&pool2)
            .await
            .expect("Pool 2 should work");
    }
}
