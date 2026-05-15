use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use ts_rs::TS;
use uuid::Uuid;

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, TS)]
pub struct ClaudeTerminalSession {
    pub execution_process_id: Uuid,
    pub workspace_id: Uuid,
    pub tmux_session_name: String,
    pub claude_session_id: Option<String>,
    pub transcript_path: Option<String>,
    pub transcript_offset: i64,
    pub cwd: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl ClaudeTerminalSession {
    pub async fn upsert_start_metadata(
        pool: &SqlitePool,
        execution_process_id: Uuid,
        workspace_id: Uuid,
        tmux_session_name: &str,
        claude_session_id: Option<&str>,
        transcript_offset: i64,
    ) -> Result<(), sqlx::Error> {
        let now = Utc::now();
        sqlx::query(
            r#"
            INSERT INTO claude_terminal_sessions (
                execution_process_id,
                workspace_id,
                tmux_session_name,
                claude_session_id,
                transcript_offset,
                created_at,
                updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
            ON CONFLICT(execution_process_id) DO UPDATE SET
                workspace_id = excluded.workspace_id,
                tmux_session_name = excluded.tmux_session_name,
                claude_session_id = COALESCE(excluded.claude_session_id, claude_terminal_sessions.claude_session_id),
                transcript_offset = excluded.transcript_offset,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(execution_process_id)
        .bind(workspace_id)
        .bind(tmux_session_name)
        .bind(claude_session_id)
        .bind(transcript_offset.max(0))
        .bind(now)
        .execute(pool)
        .await?;

        Ok(())
    }

    pub async fn upsert_from_hook(
        pool: &SqlitePool,
        execution_process_id: Uuid,
        workspace_id: Uuid,
        tmux_session_name: &str,
        claude_session_id: &str,
        transcript_path: Option<&str>,
        cwd: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        let now = Utc::now();
        sqlx::query!(
            r#"
            INSERT INTO claude_terminal_sessions (
                execution_process_id,
                workspace_id,
                tmux_session_name,
                claude_session_id,
                transcript_path,
                cwd,
                created_at,
                updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $7)
            ON CONFLICT(execution_process_id) DO UPDATE SET
                workspace_id = excluded.workspace_id,
                tmux_session_name = excluded.tmux_session_name,
                claude_session_id = excluded.claude_session_id,
                transcript_path = COALESCE(excluded.transcript_path, claude_terminal_sessions.transcript_path),
                cwd = COALESCE(excluded.cwd, claude_terminal_sessions.cwd),
                updated_at = excluded.updated_at
            "#,
            execution_process_id,
            workspace_id,
            tmux_session_name,
            claude_session_id,
            transcript_path,
            cwd,
            now
        )
        .execute(pool)
        .await?;

        Ok(())
    }

    pub async fn update_transcript_offset(
        pool: &SqlitePool,
        execution_process_id: Uuid,
        transcript_offset: i64,
    ) -> Result<(), sqlx::Error> {
        let now = Utc::now();
        sqlx::query!(
            r#"
            UPDATE claude_terminal_sessions
            SET transcript_offset = $1, updated_at = $2
            WHERE execution_process_id = $3
            "#,
            transcript_offset,
            now,
            execution_process_id
        )
        .execute(pool)
        .await?;

        Ok(())
    }

    pub async fn find_latest_for_workspace(
        pool: &SqlitePool,
        workspace_id: Uuid,
    ) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as!(
            ClaudeTerminalSession,
            r#"
            SELECT
                execution_process_id as "execution_process_id!: Uuid",
                workspace_id as "workspace_id!: Uuid",
                tmux_session_name,
                claude_session_id,
                transcript_path,
                transcript_offset as "transcript_offset!: i64",
                cwd,
                created_at as "created_at!: DateTime<Utc>",
                updated_at as "updated_at!: DateTime<Utc>"
            FROM claude_terminal_sessions
            WHERE workspace_id = $1
            ORDER BY updated_at DESC
            LIMIT 1
            "#,
            workspace_id
        )
        .fetch_optional(pool)
        .await
    }

    pub async fn find_latest_for_workspace_session(
        pool: &SqlitePool,
        workspace_id: Uuid,
        claude_session_id: &str,
    ) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as::<_, ClaudeTerminalSession>(
            r#"
            SELECT
                execution_process_id,
                workspace_id,
                tmux_session_name,
                claude_session_id,
                transcript_path,
                transcript_offset,
                cwd,
                created_at,
                updated_at
            FROM claude_terminal_sessions
            WHERE workspace_id = ?1 AND claude_session_id = ?2
            ORDER BY updated_at DESC
            LIMIT 1
            "#,
        )
        .bind(workspace_id)
        .bind(claude_session_id)
        .fetch_optional(pool)
        .await
    }

    pub async fn find_by_execution_process(
        pool: &SqlitePool,
        execution_process_id: Uuid,
    ) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as::<_, ClaudeTerminalSession>(
            r#"
            SELECT
                execution_process_id,
                workspace_id,
                tmux_session_name,
                claude_session_id,
                transcript_path,
                transcript_offset,
                cwd,
                created_at,
                updated_at
            FROM claude_terminal_sessions
            WHERE execution_process_id = ?1
            "#,
        )
        .bind(execution_process_id)
        .fetch_optional(pool)
        .await
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use sqlx::{
        SqlitePool,
        sqlite::{SqliteConnectOptions, SqlitePoolOptions},
    };

    use super::*;

    async fn setup_pool() -> SqlitePool {
        let options = SqliteConnectOptions::from_str("sqlite::memory:")
            .unwrap()
            .create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .unwrap();
        sqlx::query("PRAGMA foreign_keys = ON")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            r#"
            CREATE TABLE execution_processes (
                id BLOB PRIMARY KEY
            );
            "#,
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            r#"
            CREATE TABLE workspaces (
                id BLOB PRIMARY KEY
            );
            "#,
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            r#"
            CREATE TABLE claude_terminal_sessions (
                execution_process_id BLOB PRIMARY KEY NOT NULL,
                workspace_id         BLOB NOT NULL,
                tmux_session_name    TEXT NOT NULL,
                claude_session_id    TEXT,
                transcript_path      TEXT,
                transcript_offset    INTEGER NOT NULL DEFAULT 0,
                cwd                  TEXT,
                created_at           TEXT NOT NULL DEFAULT (datetime('now', 'subsec')),
                updated_at           TEXT NOT NULL DEFAULT (datetime('now', 'subsec')),
                FOREIGN KEY (execution_process_id) REFERENCES execution_processes(id) ON DELETE CASCADE,
                FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE
            );
            "#,
        )
        .execute(&pool)
        .await
        .unwrap();
        pool
    }

    #[tokio::test]
    async fn upsert_and_find_latest_for_workspace() {
        let pool = setup_pool().await;
        let execution_id = Uuid::new_v4();
        let workspace_id = Uuid::new_v4();
        sqlx::query("INSERT INTO execution_processes (id) VALUES (?)")
            .bind(execution_id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO workspaces (id) VALUES (?)")
            .bind(workspace_id)
            .execute(&pool)
            .await
            .unwrap();

        ClaudeTerminalSession::upsert_from_hook(
            &pool,
            execution_id,
            workspace_id,
            "vk-claude-test",
            "claude-session-1",
            Some("/tmp/transcript.jsonl"),
            Some("/tmp/worktree"),
        )
        .await
        .unwrap();
        ClaudeTerminalSession::update_transcript_offset(&pool, execution_id, 42)
            .await
            .unwrap();

        let session = ClaudeTerminalSession::find_latest_for_workspace(&pool, workspace_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(session.execution_process_id, execution_id);
        assert_eq!(session.tmux_session_name, "vk-claude-test");
        assert_eq!(
            session.claude_session_id.as_deref(),
            Some("claude-session-1")
        );
        assert_eq!(session.transcript_offset, 42);
    }

    #[tokio::test]
    async fn start_metadata_seeds_resume_transcript_offset() {
        let pool = setup_pool().await;
        let execution_id = Uuid::new_v4();
        let workspace_id = Uuid::new_v4();
        sqlx::query("INSERT INTO execution_processes (id) VALUES (?)")
            .bind(execution_id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO workspaces (id) VALUES (?)")
            .bind(workspace_id)
            .execute(&pool)
            .await
            .unwrap();

        ClaudeTerminalSession::upsert_start_metadata(
            &pool,
            execution_id,
            workspace_id,
            "vk-claude-test",
            Some("claude-session-1"),
            123,
        )
        .await
        .unwrap();

        let session = ClaudeTerminalSession::find_by_execution_process(&pool, execution_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(session.transcript_offset, 123);
        assert_eq!(
            session.claude_session_id.as_deref(),
            Some("claude-session-1")
        );
    }

    #[tokio::test]
    async fn find_latest_for_workspace_session_uses_explicit_claude_session_id() {
        let pool = setup_pool().await;
        let workspace_id = Uuid::new_v4();
        let target_execution_id = Uuid::new_v4();
        let other_execution_id = Uuid::new_v4();
        for execution_id in [target_execution_id, other_execution_id] {
            sqlx::query("INSERT INTO execution_processes (id) VALUES (?)")
                .bind(execution_id)
                .execute(&pool)
                .await
                .unwrap();
        }
        sqlx::query("INSERT INTO workspaces (id) VALUES (?)")
            .bind(workspace_id)
            .execute(&pool)
            .await
            .unwrap();

        ClaudeTerminalSession::upsert_start_metadata(
            &pool,
            target_execution_id,
            workspace_id,
            "vk-claude-target",
            Some("claude-session-target"),
            123,
        )
        .await
        .unwrap();
        ClaudeTerminalSession::upsert_start_metadata(
            &pool,
            other_execution_id,
            workspace_id,
            "vk-claude-other",
            Some("claude-session-other"),
            456,
        )
        .await
        .unwrap();

        let session = ClaudeTerminalSession::find_latest_for_workspace_session(
            &pool,
            workspace_id,
            "claude-session-target",
        )
        .await
        .unwrap()
        .unwrap();

        assert_eq!(session.execution_process_id, target_execution_id);
        assert_eq!(session.transcript_offset, 123);
    }
}
