//! Cleanup operations for old log entries.
//!
//! Functions for counting and deleting log entries older than a specified number of days.
//! This is independent of task archiving - logs can be purged regardless of task status.

use chrono::{Duration, Utc};
use sqlx::SqlitePool;

use super::DbLogEntry;

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

    /// Delete log entries older than the specified number of days.
    /// Returns the number of log entries deleted.
    pub async fn delete_older_than(pool: &SqlitePool, days: i64) -> Result<i64, sqlx::Error> {
        let cutoff = Utc::now() - Duration::days(days);
        let result = sqlx::query!("DELETE FROM log_entries WHERE timestamp < ?", cutoff)
            .execute(pool)
            .await?;
        Ok(result.rows_affected() as i64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::log_entry::{tests::create_test_execution, tests::setup_test_pool, CreateLogEntry};

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
}
