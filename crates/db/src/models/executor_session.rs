use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use ts_rs::TS;
use uuid::Uuid;

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, TS)]
pub struct ExecutorSession {
    pub id: Uuid,
    pub task_attempt_id: Uuid,
    pub execution_process_id: Uuid,
    pub session_id: Option<String>, // External session ID from Claude/Amp
    pub prompt: Option<String>,     // The prompt sent to the executor
    pub summary: Option<String>,    // Final assistant message/summary
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize, TS)]
pub struct CreateExecutorSession {
    pub task_attempt_id: Uuid,
    pub execution_process_id: Uuid,
    pub prompt: Option<String>,
}

#[derive(Debug, Deserialize, TS)]
#[allow(dead_code)]
pub struct UpdateExecutorSession {
    pub session_id: Option<String>,
    pub prompt: Option<String>,
    pub summary: Option<String>,
}

impl ExecutorSession {
    /// Find executor session by ID
    #[allow(dead_code)]
    pub async fn find_by_id(pool: &SqlitePool, id: Uuid) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as!(
            ExecutorSession,
            r#"SELECT 
                id as "id!: Uuid", 
                task_attempt_id as "task_attempt_id!: Uuid", 
                execution_process_id as "execution_process_id!: Uuid", 
                session_id, 
                prompt,
                summary,
                created_at as "created_at!: DateTime<Utc>", 
                updated_at as "updated_at!: DateTime<Utc>"
               FROM executor_sessions 
               WHERE id = $1"#,
            id
        )
        .fetch_optional(pool)
        .await
    }

    /// Find executor session by execution process ID
    pub async fn find_by_execution_process_id(
        pool: &SqlitePool,
        execution_process_id: Uuid,
    ) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as!(
            ExecutorSession,
            r#"SELECT
                id as "id!: Uuid",
                task_attempt_id as "task_attempt_id!: Uuid",
                execution_process_id as "execution_process_id!: Uuid",
                session_id,
                prompt,
                summary,
                created_at as "created_at!: DateTime<Utc>",
                updated_at as "updated_at!: DateTime<Utc>"
               FROM executor_sessions
               WHERE execution_process_id = $1"#,
            execution_process_id
        )
        .fetch_optional(pool)
        .await
    }

    /// Find all executor sessions for a task attempt
    #[allow(dead_code)]
    pub async fn find_by_task_attempt_id(
        pool: &SqlitePool,
        task_attempt_id: Uuid,
    ) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as!(
            ExecutorSession,
            r#"SELECT 
                id as "id!: Uuid", 
                task_attempt_id as "task_attempt_id!: Uuid", 
                execution_process_id as "execution_process_id!: Uuid", 
                session_id, 
                prompt,
                summary,
                created_at as "created_at!: DateTime<Utc>", 
                updated_at as "updated_at!: DateTime<Utc>"
               FROM executor_sessions 
               WHERE task_attempt_id = $1 
               ORDER BY created_at ASC"#,
            task_attempt_id
        )
        .fetch_all(pool)
        .await
    }

    pub async fn find_by_session_id(
        pool: &SqlitePool,
        session_id: &str,
    ) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as!(
            ExecutorSession,
            r#"SELECT
                id as "id!: Uuid",
                task_attempt_id as "task_attempt_id!: Uuid",
                execution_process_id as "execution_process_id!: Uuid",
                session_id,
                prompt,
                summary,
                created_at as "created_at!: DateTime<Utc>",
                updated_at as "updated_at!: DateTime<Utc>"
               FROM executor_sessions
               WHERE session_id = ?
               ORDER BY updated_at DESC
               LIMIT 1"#,
            session_id
        )
        .fetch_optional(pool)
        .await
    }

    /// Create a new executor session
    pub async fn create(
        pool: &SqlitePool,
        data: &CreateExecutorSession,
        session_id: Uuid,
    ) -> Result<Self, sqlx::Error> {
        let now = Utc::now();

        tracing::debug!(
            "Creating executor session: id={}, task_attempt_id={}, execution_process_id={}, external_session_id=None (will be set later)",
            session_id,
            data.task_attempt_id,
            data.execution_process_id
        );

        sqlx::query_as!(
            ExecutorSession,
            r#"INSERT INTO executor_sessions (
                id, task_attempt_id, execution_process_id, session_id, prompt, summary,
                created_at, updated_at
               )
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
               RETURNING
                id as "id!: Uuid",
                task_attempt_id as "task_attempt_id!: Uuid",
                execution_process_id as "execution_process_id!: Uuid",
                session_id,
                prompt,
                summary,
                created_at as "created_at!: DateTime<Utc>",
                updated_at as "updated_at!: DateTime<Utc>""#,
            session_id,
            data.task_attempt_id,
            data.execution_process_id,
            None::<String>, // session_id initially None until parsed from output
            data.prompt,
            None::<String>, // summary initially None
            now,            // created_at
            now             // updated_at
        )
        .fetch_one(pool)
        .await
    }

    /// Update executor session with external session ID
    pub async fn update_session_id(
        pool: &SqlitePool,
        execution_process_id: Uuid,
        external_session_id: &str,
    ) -> Result<(), sqlx::Error> {
        let now = Utc::now();
        sqlx::query!(
            r#"UPDATE executor_sessions
               SET session_id = $1, updated_at = $2
               WHERE execution_process_id = $3"#,
            external_session_id,
            now,
            execution_process_id
        )
        .execute(pool)
        .await?;

        Ok(())
    }

    /// Mark a session as invalid by clearing its session_id
    /// Used when session resume fails (e.g., "No conversation found" error)
    pub async fn mark_session_invalid(
        pool: &SqlitePool,
        session_id: &str,
    ) -> Result<(), sqlx::Error> {
        let now = Utc::now();
        sqlx::query!(
            r#"UPDATE executor_sessions
               SET session_id = NULL, updated_at = $1
               WHERE session_id = $2"#,
            now,
            session_id
        )
        .execute(pool)
        .await?;

        Ok(())
    }

    /// Invalidate all sessions for a task attempt so they won't be tried again. This clears sessions from ALL execution processes (not just failed ones), because session files can also become corrupted after successful completions.
    /// Returns the list of session IDs that were invalidated
    pub async fn invalidate_failed_sessions(
        pool: &SqlitePool,
        task_attempt_id: Uuid,
    ) -> Result<Vec<String>, sqlx::Error> {
        let sessions = sqlx::query!(
            r#"SELECT es.session_id
               FROM executor_sessions es
               JOIN execution_processes ep ON es.execution_process_id = ep.id
               WHERE ep.task_attempt_id = $1
                 AND es.session_id IS NOT NULL"#,
            task_attempt_id
        )
        .fetch_all(pool)
        .await?;

        let session_ids: Vec<String> = sessions
            .iter()
            .filter_map(|r| r.session_id.clone())
            .collect();

        for session_id in &session_ids {
            Self::mark_session_invalid(pool, session_id).await?;
        }

        Ok(session_ids)
    }

    /// Count sessions from failed/killed execution processes that can still be fixed
    /// (i.e., have session_id IS NOT NULL)
    pub async fn count_fixable_sessions(
        pool: &SqlitePool,
        task_attempt_id: Uuid,
    ) -> Result<i64, sqlx::Error> {
        let result = sqlx::query!(
            r#"SELECT COUNT(*) as count
               FROM executor_sessions es
               JOIN execution_processes ep ON es.execution_process_id = ep.id
               WHERE ep.task_attempt_id = $1
                 AND ep.status IN ('failed', 'killed')
                 AND es.session_id IS NOT NULL"#,
            task_attempt_id
        )
        .fetch_one(pool)
        .await?;

        Ok(result.count)
    }

    /// Update executor session prompt
    #[allow(dead_code)]
    pub async fn update_prompt(
        pool: &SqlitePool,
        id: Uuid,
        prompt: &str,
    ) -> Result<(), sqlx::Error> {
        let now = Utc::now();
        sqlx::query!(
            r#"UPDATE executor_sessions 
               SET prompt = $1, updated_at = $2 
               WHERE id = $3"#,
            prompt,
            now,
            id
        )
        .execute(pool)
        .await?;

        Ok(())
    }

    /// Update executor session summary
    pub async fn update_summary(
        pool: &SqlitePool,
        execution_process_id: Uuid,
        summary: &str,
    ) -> Result<(), sqlx::Error> {
        let now = Utc::now();
        sqlx::query!(
            r#"UPDATE executor_sessions 
               SET summary = $1, updated_at = $2 
               WHERE execution_process_id = $3"#,
            summary,
            now,
            execution_process_id
        )
        .execute(pool)
        .await?;

        Ok(())
    }

    /// Delete executor sessions for a task attempt (cleanup)
    pub async fn delete_by_task_attempt_id(
        pool: &SqlitePool,
        task_attempt_id: Uuid,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "DELETE FROM executor_sessions WHERE task_attempt_id = $1",
            task_attempt_id
        )
        .execute(pool)
        .await?;

        Ok(())
    }
}
