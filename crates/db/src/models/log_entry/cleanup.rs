//! Cleanup operations for old log entries.
//!
//! Functions for counting and deleting log entries older than a specified number of days.
//! This is independent of task archiving - logs can be purged regardless of task status.

use chrono::{Duration, Utc};
use sqlx::SqlitePool;
use std::time::Duration as StdDuration;

use super::DbLogEntry;

/// Maximum number of rows to delete in a single batch.
/// Prevents long database locks on large purge operations.
const BATCH_SIZE: i64 = 10_000;

/// Milliseconds to wait between batches.
/// Allows other database operations to proceed during large purges.
const YIELD_DURATION_MS: u64 = 10;

impl DbLogEntry {
    /// Count log entries older than the specified number of days.
    /// Returns the count of log entries that would be purged.
    pub async fn count_older_than(pool: &SqlitePool, days: i64) -> Result<i64, sqlx::Error> {
        let cutoff = Utc::now() - Duration::days(days);
        let result = sqlx::query_scalar!(
            r#"SELECT COUNT(*) as "count!: i64" FROM log_entries WHERE timestamp < ?"#,
            cutoff
        )
        .fetch_one(pool)
        .await?;
        Ok(result)
    }

    /// Delete log entries older than the specified number of days using batched deletion.
    ///
    /// This function deletes entries in batches of 10,000 rows to prevent long database locks
    /// on large purge operations. A 10ms yield is added between batches to allow other
    /// database operations to proceed.
    ///
    /// Returns the total number of log entries deleted.
    pub async fn delete_older_than(pool: &SqlitePool, days: i64) -> Result<i64, sqlx::Error> {
        let cutoff = Utc::now() - Duration::days(days);
        let mut total_deleted: i64 = 0;

        loop {
            // Delete a batch using DELETE with LIMIT via a subquery
            let result = sqlx::query(
                r#"DELETE FROM log_entries
                   WHERE id IN (
                       SELECT id FROM log_entries
                       WHERE timestamp < ?
                       LIMIT ?
                   )"#,
            )
            .bind(cutoff)
            .bind(BATCH_SIZE)
            .execute(pool)
            .await?;

            let batch_deleted = result.rows_affected() as i64;
            total_deleted += batch_deleted;

            // If we deleted fewer rows than the batch size, we're done
            if batch_deleted < BATCH_SIZE {
                break;
            }

            // Yield to allow other database operations to proceed
            tokio::time::sleep(StdDuration::from_millis(YIELD_DURATION_MS)).await;
        }

        Ok(total_deleted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::log_entry::{
        CreateLogEntry, tests::create_test_execution, tests::setup_test_pool,
    };

    #[tokio::test]
    async fn test_count_older_than() {
        let (pool, _temp_dir) = setup_test_pool().await;
        let execution_id = create_test_execution(&pool).await;

        // Create some log entries
        DbLogEntry::create(
            &pool,
            CreateLogEntry {
                execution_id,
                output_type: "stdout".into(),
                content: "Test log 1".into(),
            },
        )
        .await
        .expect("Failed to create log entry");

        DbLogEntry::create(
            &pool,
            CreateLogEntry {
                execution_id,
                output_type: "stdout".into(),
                content: "Test log 2".into(),
            },
        )
        .await
        .expect("Failed to create log entry");

        // With 1 day cutoff, logs created just now should NOT be counted
        let count = DbLogEntry::count_older_than(&pool, 1)
            .await
            .expect("Failed to count");
        assert_eq!(count, 0);

        // With 0 days cutoff (cutoff = now), logs created just before should be counted
        let count = DbLogEntry::count_older_than(&pool, 0)
            .await
            .expect("Failed to count");
        assert_eq!(count, 2);
    }

    #[tokio::test]
    async fn test_delete_older_than() {
        let (pool, _temp_dir) = setup_test_pool().await;
        let execution_id = create_test_execution(&pool).await;

        // Create some log entries
        for i in 0..5 {
            DbLogEntry::create(
                &pool,
                CreateLogEntry {
                    execution_id,
                    output_type: "stdout".into(),
                    content: format!("Test log {}", i),
                },
            )
            .await
            .expect("Failed to create log entry");
        }

        // Verify 5 entries exist
        let entries = DbLogEntry::find_by_execution_id(&pool, execution_id)
            .await
            .expect("Query failed");
        assert_eq!(entries.len(), 5);

        // Delete with 0 days cutoff (cutoff = now)
        let deleted = DbLogEntry::delete_older_than(&pool, 0)
            .await
            .expect("Failed to delete");
        assert_eq!(deleted, 5);

        // Verify entries are gone
        let entries = DbLogEntry::find_by_execution_id(&pool, execution_id)
            .await
            .expect("Query failed");
        assert!(entries.is_empty());
    }

    #[tokio::test]
    async fn test_delete_older_than_empty_database() {
        let (pool, _temp_dir) = setup_test_pool().await;

        // Don't create any entries - database should be empty

        // Delete with 0 days cutoff on empty database
        let deleted = DbLogEntry::delete_older_than(&pool, 0)
            .await
            .expect("Failed to delete");

        // Should return 0 for empty database
        assert_eq!(deleted, 0);
    }

    #[tokio::test]
    async fn test_delete_older_than_exact_batch_size() {
        let (pool, _temp_dir) = setup_test_pool().await;
        let execution_id = create_test_execution(&pool).await;

        // Create exactly BATCH_SIZE (10,000) entries
        for i in 0..BATCH_SIZE {
            DbLogEntry::create(
                &pool,
                CreateLogEntry {
                    execution_id,
                    output_type: "stdout".into(),
                    content: format!("Test log {}", i),
                },
            )
            .await
            .expect("Failed to create log entry");
        }

        // Verify count
        let count = DbLogEntry::count_older_than(&pool, 0)
            .await
            .expect("Failed to count");
        assert_eq!(count, BATCH_SIZE);

        // Delete with 0 days cutoff
        let deleted = DbLogEntry::delete_older_than(&pool, 0)
            .await
            .expect("Failed to delete");

        // Should delete all 10,000 entries
        assert_eq!(deleted, BATCH_SIZE);

        // Verify all entries are gone
        let count_after = DbLogEntry::count_older_than(&pool, 0)
            .await
            .expect("Failed to count");
        assert_eq!(count_after, 0);
    }

    #[tokio::test]
    async fn test_delete_older_than_multiple_batches() {
        let (pool, _temp_dir) = setup_test_pool().await;
        let execution_id = create_test_execution(&pool).await;

        // Create 25,000 entries (2.5 batches)
        const ENTRY_COUNT: i64 = 25_000;
        for i in 0..ENTRY_COUNT {
            DbLogEntry::create(
                &pool,
                CreateLogEntry {
                    execution_id,
                    output_type: "stdout".into(),
                    content: format!("Test log {}", i),
                },
            )
            .await
            .expect("Failed to create log entry");
        }

        // Verify count
        let count = DbLogEntry::count_older_than(&pool, 0)
            .await
            .expect("Failed to count");
        assert_eq!(count, ENTRY_COUNT);

        // Delete with 0 days cutoff
        let deleted = DbLogEntry::delete_older_than(&pool, 0)
            .await
            .expect("Failed to delete");

        // Should delete all 25,000 entries across multiple batches
        assert_eq!(deleted, ENTRY_COUNT);

        // Verify all entries are gone
        let count_after = DbLogEntry::count_older_than(&pool, 0)
            .await
            .expect("Failed to count");
        assert_eq!(count_after, 0);
    }
}
