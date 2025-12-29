//! Repository for task output logs from node execution.

use chrono::{DateTime, Utc};
use sqlx::{FromRow, PgPool};
use thiserror::Error;
use utils::unified_log::{Direction, LogEntry, OutputType, PaginatedLogs};
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum TaskOutputLogError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}

/// A single output log entry from task execution.
#[derive(Debug, Clone, FromRow)]
pub struct TaskOutputLog {
    pub id: i64,
    pub assignment_id: Uuid,
    pub output_type: String,
    pub content: String,
    pub timestamp: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

/// Data for creating a new output log entry.
pub struct CreateTaskOutputLog {
    pub assignment_id: Uuid,
    pub output_type: String,
    pub content: String,
    pub timestamp: DateTime<Utc>,
}

pub struct TaskOutputLogRepository<'a> {
    pool: &'a PgPool,
}

impl<'a> TaskOutputLogRepository<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    /// Create a new output log entry.
    pub async fn create(
        &self,
        data: CreateTaskOutputLog,
    ) -> Result<TaskOutputLog, TaskOutputLogError> {
        let log = sqlx::query_as::<_, TaskOutputLog>(
            r#"
            INSERT INTO node_task_output_logs (assignment_id, output_type, content, timestamp)
            VALUES ($1, $2, $3, $4)
            RETURNING id, assignment_id, output_type, content, timestamp, created_at
            "#,
        )
        .bind(data.assignment_id)
        .bind(&data.output_type)
        .bind(&data.content)
        .bind(data.timestamp)
        .fetch_one(self.pool)
        .await?;

        Ok(log)
    }

    /// Create a new output log entry with an optional execution_process_id.
    pub async fn create_with_execution_process(
        &self,
        data: CreateTaskOutputLog,
        execution_process_id: Option<Uuid>,
    ) -> Result<TaskOutputLog, TaskOutputLogError> {
        let log = sqlx::query_as::<_, TaskOutputLog>(
            r#"
            INSERT INTO node_task_output_logs (assignment_id, output_type, content, timestamp, execution_process_id)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING id, assignment_id, output_type, content, timestamp, created_at
            "#,
        )
        .bind(data.assignment_id)
        .bind(&data.output_type)
        .bind(&data.content)
        .bind(data.timestamp)
        .bind(execution_process_id)
        .fetch_one(self.pool)
        .await?;

        Ok(log)
    }

    /// Create multiple output log entries in a batch.
    pub async fn create_batch(
        &self,
        entries: Vec<CreateTaskOutputLog>,
    ) -> Result<(), TaskOutputLogError> {
        if entries.is_empty() {
            return Ok(());
        }

        // Build a batch insert query
        let mut query = String::from(
            "INSERT INTO node_task_output_logs (assignment_id, output_type, content, timestamp) VALUES ",
        );

        let mut params: Vec<String> = Vec::with_capacity(entries.len());
        for (i, _) in entries.iter().enumerate() {
            let base = i * 4;
            params.push(format!(
                "(${}, ${}, ${}, ${})",
                base + 1,
                base + 2,
                base + 3,
                base + 4
            ));
        }
        query.push_str(&params.join(", "));

        let mut query_builder = sqlx::query(&query);
        for entry in &entries {
            query_builder = query_builder
                .bind(entry.assignment_id)
                .bind(&entry.output_type)
                .bind(&entry.content)
                .bind(entry.timestamp);
        }

        query_builder.execute(self.pool).await?;

        Ok(())
    }

    /// Get output logs for an assignment.
    pub async fn list_by_assignment(
        &self,
        assignment_id: Uuid,
        limit: Option<i64>,
        after_id: Option<i64>,
    ) -> Result<Vec<TaskOutputLog>, TaskOutputLogError> {
        let limit = limit.unwrap_or(1000);

        let logs = if let Some(after_id) = after_id {
            sqlx::query_as::<_, TaskOutputLog>(
                r#"
                SELECT id, assignment_id, output_type, content, timestamp, created_at
                FROM node_task_output_logs
                WHERE assignment_id = $1
                  AND id > $2
                ORDER BY id ASC
                LIMIT $3
                "#,
            )
            .bind(assignment_id)
            .bind(after_id)
            .bind(limit)
            .fetch_all(self.pool)
            .await?
        } else {
            sqlx::query_as::<_, TaskOutputLog>(
                r#"
                SELECT id, assignment_id, output_type, content, timestamp, created_at
                FROM node_task_output_logs
                WHERE assignment_id = $1
                ORDER BY id ASC
                LIMIT $2
                "#,
            )
            .bind(assignment_id)
            .bind(limit)
            .fetch_all(self.pool)
            .await?
        };

        Ok(logs)
    }

    /// Get the latest output logs (most recent first) for an assignment.
    pub async fn list_latest_by_assignment(
        &self,
        assignment_id: Uuid,
        limit: i64,
    ) -> Result<Vec<TaskOutputLog>, TaskOutputLogError> {
        let logs = sqlx::query_as::<_, TaskOutputLog>(
            r#"
            SELECT id, assignment_id, output_type, content, timestamp, created_at
            FROM node_task_output_logs
            WHERE assignment_id = $1
            ORDER BY id DESC
            LIMIT $2
            "#,
        )
        .bind(assignment_id)
        .bind(limit)
        .fetch_all(self.pool)
        .await?;

        Ok(logs)
    }

    /// Get the count of output logs for an assignment.
    pub async fn count_by_assignment(
        &self,
        assignment_id: Uuid,
    ) -> Result<i64, TaskOutputLogError> {
        let row: (i64,) = sqlx::query_as(
            r#"
            SELECT COUNT(*) as count
            FROM node_task_output_logs
            WHERE assignment_id = $1
            "#,
        )
        .bind(assignment_id)
        .fetch_one(self.pool)
        .await?;

        Ok(row.0)
    }

    /// Delete output logs for an assignment.
    pub async fn delete_by_assignment(
        &self,
        assignment_id: Uuid,
    ) -> Result<u64, TaskOutputLogError> {
        let result = sqlx::query(
            r#"
            DELETE FROM node_task_output_logs
            WHERE assignment_id = $1
            "#,
        )
        .bind(assignment_id)
        .execute(self.pool)
        .await?;

        Ok(result.rows_affected())
    }

    /// Get paginated output logs for an assignment with cursor-based pagination.
    ///
    /// Returns logs in a unified format matching the local server's pagination API.
    ///
    /// # Arguments
    /// * `assignment_id` - The assignment (execution) ID to fetch logs for
    /// * `cursor` - Entry ID to start from (exclusive). None for initial fetch.
    /// * `limit` - Maximum entries to return
    /// * `direction` - Forward (oldest first, id > cursor) or Backward (newest first, id < cursor)
    pub async fn find_paginated(
        &self,
        assignment_id: Uuid,
        cursor: Option<i64>,
        limit: i64,
        direction: Direction,
    ) -> Result<PaginatedLogs, TaskOutputLogError> {
        // Fetch one more than requested to determine if there are more entries
        let fetch_limit = limit + 1;

        let logs = match (direction, cursor) {
            // Backward (newest first): ORDER BY id DESC, fetch id < cursor
            (Direction::Backward, Some(cursor_id)) => {
                sqlx::query_as::<_, TaskOutputLog>(
                    r#"
                    SELECT id, assignment_id, output_type, content, timestamp, created_at
                    FROM node_task_output_logs
                    WHERE assignment_id = $1 AND id < $2
                    ORDER BY id DESC
                    LIMIT $3
                    "#,
                )
                .bind(assignment_id)
                .bind(cursor_id)
                .bind(fetch_limit)
                .fetch_all(self.pool)
                .await?
            }
            // Backward without cursor: newest entries first
            (Direction::Backward, None) => {
                sqlx::query_as::<_, TaskOutputLog>(
                    r#"
                    SELECT id, assignment_id, output_type, content, timestamp, created_at
                    FROM node_task_output_logs
                    WHERE assignment_id = $1
                    ORDER BY id DESC
                    LIMIT $2
                    "#,
                )
                .bind(assignment_id)
                .bind(fetch_limit)
                .fetch_all(self.pool)
                .await?
            }
            // Forward (oldest first): ORDER BY id ASC, fetch id > cursor
            (Direction::Forward, Some(cursor_id)) => {
                sqlx::query_as::<_, TaskOutputLog>(
                    r#"
                    SELECT id, assignment_id, output_type, content, timestamp, created_at
                    FROM node_task_output_logs
                    WHERE assignment_id = $1 AND id > $2
                    ORDER BY id ASC
                    LIMIT $3
                    "#,
                )
                .bind(assignment_id)
                .bind(cursor_id)
                .bind(fetch_limit)
                .fetch_all(self.pool)
                .await?
            }
            // Forward without cursor: oldest entries first
            (Direction::Forward, None) => {
                sqlx::query_as::<_, TaskOutputLog>(
                    r#"
                    SELECT id, assignment_id, output_type, content, timestamp, created_at
                    FROM node_task_output_logs
                    WHERE assignment_id = $1
                    ORDER BY id ASC
                    LIMIT $2
                    "#,
                )
                .bind(assignment_id)
                .bind(fetch_limit)
                .fetch_all(self.pool)
                .await?
            }
        };

        // Determine if there are more entries
        let has_more = logs.len() as i64 > limit;

        // Take only the requested number of entries
        let entries: Vec<LogEntry> = logs
            .into_iter()
            .take(limit as usize)
            .map(|log| {
                LogEntry::new(
                    log.id,
                    log.content,
                    OutputType::from_remote_str(&log.output_type),
                    log.timestamp,
                    log.assignment_id,
                )
            })
            .collect();

        // Calculate next cursor based on direction
        let next_cursor = if has_more {
            entries.last().map(|e| e.id)
        } else {
            None
        };

        Ok(PaginatedLogs::new(entries, next_cursor, has_more, None))
    }

    /// Get paginated logs with total count.
    ///
    /// This method also fetches the total count of logs for the assignment,
    /// useful for UI display.
    pub async fn find_paginated_with_count(
        &self,
        assignment_id: Uuid,
        cursor: Option<i64>,
        limit: i64,
        direction: Direction,
    ) -> Result<PaginatedLogs, TaskOutputLogError> {
        // Get paginated logs and count in parallel
        let (paginated, count) = tokio::try_join!(
            self.find_paginated(assignment_id, cursor, limit, direction),
            self.count_by_assignment(assignment_id)
        )?;

        Ok(paginated.with_total_count(count))
    }
}
