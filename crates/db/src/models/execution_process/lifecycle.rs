//! Lifecycle and status update operations for execution processes.

use chrono::Utc;
use sqlx::SqlitePool;
use uuid::Uuid;

use super::{ExecutionProcess, ExecutionProcessStatus};

impl ExecutionProcess {
    pub async fn was_stopped(pool: &SqlitePool, id: Uuid) -> bool {
        if let Ok(exp_process) = Self::find_by_id(pool, id).await
            && exp_process.is_some_and(|ep| {
                ep.status == ExecutionProcessStatus::Killed
                    || ep.status == ExecutionProcessStatus::Completed
            })
        {
            return true;
        }
        false
    }

    /// Update execution process status and completion info
    pub async fn update_completion(
        pool: &SqlitePool,
        id: Uuid,
        status: ExecutionProcessStatus,
        exit_code: Option<i64>,
    ) -> Result<(), sqlx::Error> {
        let completed_at = if matches!(status, ExecutionProcessStatus::Running) {
            None
        } else {
            Some(Utc::now())
        };

        sqlx::query!(
            r#"UPDATE execution_processes
               SET status = $1, exit_code = $2, completed_at = $3
               WHERE id = $4"#,
            status,
            exit_code,
            completed_at,
            id
        )
        .execute(pool)
        .await?;

        Ok(())
    }

    /// Update the "after" commit oid for the process
    pub async fn update_after_head_commit(
        pool: &SqlitePool,
        id: Uuid,
        after_head_commit: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"UPDATE execution_processes
               SET after_head_commit = $1
               WHERE id = $2"#,
            after_head_commit,
            id
        )
        .execute(pool)
        .await?;
        Ok(())
    }

    /// Update the "before" commit oid for the process
    pub async fn update_before_head_commit(
        pool: &SqlitePool,
        id: Uuid,
        before_head_commit: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"UPDATE execution_processes
               SET before_head_commit = $1
               WHERE id = $2"#,
            before_head_commit,
            id
        )
        .execute(pool)
        .await?;
        Ok(())
    }

    /// Update the system process ID (PID) for process tree discovery
    pub async fn update_pid(pool: &SqlitePool, id: Uuid, pid: i64) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"UPDATE execution_processes
               SET pid = $1
               WHERE id = $2"#,
            pid,
            id
        )
        .execute(pool)
        .await?;
        Ok(())
    }

    /// Set restore boundary: drop processes newer than the specified process, undrop older/equal
    pub async fn set_restore_boundary(
        pool: &SqlitePool,
        task_attempt_id: Uuid,
        boundary_process_id: Uuid,
    ) -> Result<(), sqlx::Error> {
        // Monotonic drop: only mark newer records as dropped; never undrop.
        sqlx::query!(
            r#"UPDATE execution_processes
               SET dropped = TRUE
             WHERE task_attempt_id = $1
               AND created_at > (SELECT created_at FROM execution_processes WHERE id = $2)
               AND dropped = FALSE
            "#,
            task_attempt_id,
            boundary_process_id
        )
        .execute(pool)
        .await?;
        Ok(())
    }

    /// Soft-drop processes at and after the specified boundary (inclusive)
    pub async fn drop_at_and_after(
        pool: &SqlitePool,
        task_attempt_id: Uuid,
        boundary_process_id: Uuid,
    ) -> Result<i64, sqlx::Error> {
        let result = sqlx::query!(
            r#"UPDATE execution_processes
               SET dropped = TRUE
             WHERE task_attempt_id = $1
               AND created_at >= (SELECT created_at FROM execution_processes WHERE id = $2)
               AND dropped = FALSE"#,
            task_attempt_id,
            boundary_process_id
        )
        .execute(pool)
        .await?;
        Ok(result.rows_affected() as i64)
    }
}
