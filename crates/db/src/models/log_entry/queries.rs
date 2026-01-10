//! CRUD operations for log entries.

use chrono::{DateTime, Utc};
use sqlx::SqlitePool;
use uuid::Uuid;

use super::{CreateLogEntry, DbLogEntry};

impl DbLogEntry {
    /// Create a new log entry in the database.
    pub async fn create(pool: &SqlitePool, data: CreateLogEntry) -> Result<Self, sqlx::Error> {
        let row = sqlx::query_as!(
            DbLogEntry,
            r#"INSERT INTO log_entries (execution_id, output_type, content, timestamp)
               VALUES ($1, $2, $3, datetime('now', 'subsec'))
               RETURNING
                   id as "id!",
                   execution_id as "execution_id!: Uuid",
                   output_type,
                   content,
                   timestamp as "timestamp!: DateTime<Utc>",
                   hive_synced_at as "hive_synced_at: DateTime<Utc>""#,
            data.execution_id,
            data.output_type,
            data.content
        )
        .fetch_one(pool)
        .await?;

        Ok(row)
    }

    /// Find a log entry by ID.
    pub async fn find_by_id(pool: &SqlitePool, id: i64) -> Result<Option<Self>, sqlx::Error> {
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
               WHERE id = $1"#,
            id
        )
        .fetch_optional(pool)
        .await
    }

    /// Find all log entries for an execution process.
    pub async fn find_by_execution_id(
        pool: &SqlitePool,
        execution_id: Uuid,
    ) -> Result<Vec<Self>, sqlx::Error> {
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
               ORDER BY id ASC"#,
            execution_id
        )
        .fetch_all(pool)
        .await
    }

    /// Find all log entries for an execution process after a given timestamp.
    pub async fn find_by_execution_id_after(
        pool: &SqlitePool,
        execution_id: Uuid,
        after: DateTime<Utc>,
    ) -> Result<Vec<Self>, sqlx::Error> {
        // Format the timestamp to match SQLite's datetime format (YYYY-MM-DD HH:MM:SS.SSS)
        let after_str = after.format("%Y-%m-%d %H:%M:%S%.3f").to_string();
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
               WHERE execution_id = $1 AND timestamp > $2
               ORDER BY id ASC"#,
            execution_id,
            after_str
        )
        .fetch_all(pool)
        .await
    }

    /// Delete all log entries for an execution process.
    pub async fn delete_by_execution_id(
        pool: &SqlitePool,
        execution_id: Uuid,
    ) -> Result<u64, sqlx::Error> {
        let result = sqlx::query!(
            "DELETE FROM log_entries WHERE execution_id = $1",
            execution_id
        )
        .execute(pool)
        .await?;

        Ok(result.rows_affected())
    }
}
