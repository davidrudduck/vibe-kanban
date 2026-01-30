//! Hive sync operations for log entries.

use chrono::Utc;
use sqlx::SqlitePool;
use uuid::Uuid;

use super::DbLogEntry;

impl DbLogEntry {
    /// Find log entries that have not been synced to the Hive.
    /// Returns entries grouped by execution_id and ordered by id (oldest first).
    /// This allows batching log entries for efficient sync.
    /// Only returns entries whose parent execution has been synced,
    /// to avoid FK constraint errors on the server side.
    pub async fn find_unsynced(pool: &SqlitePool, limit: i64) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as::<_, DbLogEntry>(
            r#"SELECT
                le.id,
                le.execution_id,
                le.output_type,
                le.content,
                le.timestamp,
                le.hive_synced_at
               FROM log_entries le
               INNER JOIN execution_processes ep ON le.execution_id = ep.id
               WHERE le.hive_synced_at IS NULL
                 AND ep.hive_synced_at IS NOT NULL
               ORDER BY le.execution_id, le.id ASC
               LIMIT ?"#,
        )
        .bind(limit)
        .fetch_all(pool)
        .await
    }

    /// Mark a log entry as synced to the Hive.
    pub async fn mark_hive_synced(pool: &SqlitePool, id: i64) -> Result<(), sqlx::Error> {
        let now = Utc::now();
        sqlx::query!(
            "UPDATE log_entries SET hive_synced_at = $1 WHERE id = $2",
            now,
            id
        )
        .execute(pool)
        .await?;
        Ok(())
    }

    /// Mark multiple log entries as synced to the Hive.
    pub async fn mark_hive_synced_batch(
        pool: &SqlitePool,
        ids: &[i64],
    ) -> Result<u64, sqlx::Error> {
        if ids.is_empty() {
            return Ok(0);
        }

        let now = Utc::now();
        let placeholders: Vec<String> = (1..=ids.len()).map(|i| format!("${}", i + 1)).collect();
        let query = format!(
            "UPDATE log_entries SET hive_synced_at = $1 WHERE id IN ({})",
            placeholders.join(", ")
        );

        let mut query_builder = sqlx::query(&query).bind(now);
        for id in ids {
            query_builder = query_builder.bind(id);
        }

        let result = query_builder.execute(pool).await?;
        Ok(result.rows_affected())
    }

    /// Count log entries that have not been synced to the Hive.
    /// Useful for monitoring sync status.
    pub async fn count_unsynced(pool: &SqlitePool) -> Result<i64, sqlx::Error> {
        let result = sqlx::query!(
            r#"SELECT COUNT(*) as "count!" FROM log_entries WHERE hive_synced_at IS NULL"#
        )
        .fetch_one(pool)
        .await?;
        Ok(result.count)
    }

    /// Clear hive_synced_at for all log entries belonging to executions in a project.
    /// This triggers them to be re-synced on the next sync cycle.
    pub async fn clear_hive_sync_for_project(
        pool: &SqlitePool,
        project_id: Uuid,
    ) -> Result<u64, sqlx::Error> {
        let result = sqlx::query!(
            r#"UPDATE log_entries
               SET hive_synced_at = NULL
               WHERE execution_id IN (
                   SELECT ep.id FROM execution_processes ep
                   JOIN task_attempts ta ON ep.task_attempt_id = ta.id
                   JOIN tasks t ON ta.task_id = t.id
                   WHERE t.project_id = $1
               )"#,
            project_id
        )
        .execute(pool)
        .await?;
        Ok(result.rows_affected())
    }
}
