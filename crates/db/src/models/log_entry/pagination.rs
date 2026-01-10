//! Pagination logic for log entries.

use chrono::{DateTime, Utc};
use sqlx::SqlitePool;
use utils::unified_log::Direction;
use uuid::Uuid;

use super::{DbLogEntry, PaginatedDbLogEntries};

impl DbLogEntry {
    /// Find paginated log entries for an execution process.
    ///
    /// # Arguments
    /// * `pool` - Database connection pool
    /// * `execution_id` - The execution process ID to fetch logs for
    /// * `cursor` - Optional cursor (entry ID) to start from
    /// * `limit` - Maximum number of entries to return
    /// * `direction` - Forward (oldest first) or Backward (newest first)
    ///
    /// # Returns
    /// A `PaginatedDbLogEntries` struct containing the entries and pagination info.
    pub async fn find_paginated(
        pool: &SqlitePool,
        execution_id: Uuid,
        cursor: Option<i64>,
        limit: i64,
        direction: Direction,
    ) -> Result<PaginatedDbLogEntries, sqlx::Error> {
        // Get total count first
        let total_count: i64 = sqlx::query_scalar!(
            r#"SELECT COUNT(*) as "count!: i64" FROM log_entries WHERE execution_id = $1"#,
            execution_id
        )
        .fetch_one(pool)
        .await?;

        if total_count == 0 {
            return Ok(PaginatedDbLogEntries::empty());
        }

        // Fetch one extra to determine has_more
        let fetch_limit = limit + 1;

        let entries = match direction {
            Direction::Forward => {
                if let Some(cursor_id) = cursor {
                    sqlx::query_as!(
                        DbLogEntry,
                        r#"SELECT
                            id as "id!",
                            execution_id as "execution_id!: Uuid",
                            output_type,
                            content,
                            timestamp as "timestamp!: DateTime<Utc>",
                            hive_synced_at as "hive_synced_at: DateTime<Utc>"
                           FROM log_entries
                           WHERE execution_id = $1 AND id > $2
                           ORDER BY id ASC
                           LIMIT $3"#,
                        execution_id,
                        cursor_id,
                        fetch_limit
                    )
                    .fetch_all(pool)
                    .await?
                } else {
                    sqlx::query_as!(
                        DbLogEntry,
                        r#"SELECT
                            id as "id!",
                            execution_id as "execution_id!: Uuid",
                            output_type,
                            content,
                            timestamp as "timestamp!: DateTime<Utc>",
                            hive_synced_at as "hive_synced_at: DateTime<Utc>"
                           FROM log_entries
                           WHERE execution_id = $1
                           ORDER BY id ASC
                           LIMIT $2"#,
                        execution_id,
                        fetch_limit
                    )
                    .fetch_all(pool)
                    .await?
                }
            }
            Direction::Backward => {
                if let Some(cursor_id) = cursor {
                    sqlx::query_as!(
                        DbLogEntry,
                        r#"SELECT
                            id as "id!",
                            execution_id as "execution_id!: Uuid",
                            output_type,
                            content,
                            timestamp as "timestamp!: DateTime<Utc>",
                            hive_synced_at as "hive_synced_at: DateTime<Utc>"
                           FROM log_entries
                           WHERE execution_id = $1 AND id < $2
                           ORDER BY id DESC
                           LIMIT $3"#,
                        execution_id,
                        cursor_id,
                        fetch_limit
                    )
                    .fetch_all(pool)
                    .await?
                } else {
                    sqlx::query_as!(
                        DbLogEntry,
                        r#"SELECT
                            id as "id!",
                            execution_id as "execution_id!: Uuid",
                            output_type,
                            content,
                            timestamp as "timestamp!: DateTime<Utc>",
                            hive_synced_at as "hive_synced_at: DateTime<Utc>"
                           FROM log_entries
                           WHERE execution_id = $1
                           ORDER BY id DESC
                           LIMIT $2"#,
                        execution_id,
                        fetch_limit
                    )
                    .fetch_all(pool)
                    .await?
                }
            }
        };

        let has_more = entries.len() > limit as usize;
        let entries: Vec<DbLogEntry> = entries.into_iter().take(limit as usize).collect();

        let next_cursor = if has_more {
            entries.last().map(|e| e.id)
        } else {
            None
        };

        Ok(PaginatedDbLogEntries {
            entries,
            next_cursor,
            has_more,
            total_count: Some(total_count),
        })
    }
}
