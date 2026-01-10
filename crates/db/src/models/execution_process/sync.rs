//! Hive sync operations for execution processes.

use chrono::Utc;
use sqlx::SqlitePool;
use uuid::Uuid;

use super::ExecutionProcess;

impl ExecutionProcess {
    /// Find execution processes that have not been synced to the Hive.
    /// Returns processes ordered by created_at (oldest first) for incremental sync.
    /// Only returns executions whose parent attempt has already been synced,
    /// to avoid FK constraint errors on the server side.
    pub async fn find_unsynced(pool: &SqlitePool, limit: i64) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as::<_, ExecutionProcess>(
            r#"SELECT ep.id, ep.task_attempt_id, ep.run_reason, ep.executor_action, ep.before_head_commit,
                      ep.after_head_commit, ep.status, ep.exit_code, ep.dropped, ep.pid, ep.started_at, ep.completed_at,
                      ep.created_at, ep.updated_at, ep.hive_synced_at
               FROM execution_processes ep
               INNER JOIN task_attempts ta ON ep.task_attempt_id = ta.id
               WHERE ep.hive_synced_at IS NULL
                 AND ta.hive_synced_at IS NOT NULL
               ORDER BY ep.created_at ASC
               LIMIT ?"#,
        )
        .bind(limit)
        .fetch_all(pool)
        .await
    }

    /// Mark an execution process as synced to the Hive.
    pub async fn mark_hive_synced(pool: &SqlitePool, id: Uuid) -> Result<(), sqlx::Error> {
        let now = Utc::now();
        sqlx::query!(
            "UPDATE execution_processes SET hive_synced_at = $1 WHERE id = $2",
            now,
            id
        )
        .execute(pool)
        .await?;
        Ok(())
    }

    /// Mark multiple execution processes as synced to the Hive.
    pub async fn mark_hive_synced_batch(
        pool: &SqlitePool,
        ids: &[Uuid],
    ) -> Result<u64, sqlx::Error> {
        if ids.is_empty() {
            return Ok(0);
        }

        let now = Utc::now();
        let placeholders: Vec<String> = (1..=ids.len()).map(|i| format!("${}", i + 1)).collect();
        let query = format!(
            "UPDATE execution_processes SET hive_synced_at = $1 WHERE id IN ({})",
            placeholders.join(", ")
        );

        let mut query_builder = sqlx::query(&query).bind(now);
        for id in ids {
            query_builder = query_builder.bind(id);
        }

        let result = query_builder.execute(pool).await?;
        Ok(result.rows_affected())
    }
}
