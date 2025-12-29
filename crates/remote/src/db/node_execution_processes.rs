//! Repository for node execution processes synced from nodes to the Hive.

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use thiserror::Error;
use uuid::Uuid;

use crate::nodes::NodeExecutionProcess;

#[derive(Debug, Error)]
pub enum NodeExecutionProcessError {
    #[error("node execution process not found")]
    NotFound,
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}

/// Data for creating or upserting a node execution process
#[derive(Debug, Clone)]
pub struct UpsertNodeExecutionProcess {
    pub id: Uuid,
    pub attempt_id: Uuid,
    pub node_id: Uuid,
    pub run_reason: String,
    pub executor_action: Option<serde_json::Value>,
    pub before_head_commit: Option<String>,
    pub after_head_commit: Option<String>,
    pub status: String,
    pub exit_code: Option<i32>,
    pub dropped: bool,
    pub pid: Option<i64>,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

pub struct NodeExecutionProcessRepository<'a> {
    pool: &'a PgPool,
}

impl<'a> NodeExecutionProcessRepository<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    /// Upsert a node execution process (insert or update on conflict)
    pub async fn upsert(
        &self,
        data: &UpsertNodeExecutionProcess,
    ) -> Result<NodeExecutionProcess, NodeExecutionProcessError> {
        let process = sqlx::query_as::<_, NodeExecutionProcess>(
            r#"
            INSERT INTO node_execution_processes (
                id, attempt_id, node_id, run_reason, executor_action,
                before_head_commit, after_head_commit, status, exit_code,
                dropped, pid, started_at, completed_at, created_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
            ON CONFLICT (id) DO UPDATE SET
                run_reason = EXCLUDED.run_reason,
                executor_action = EXCLUDED.executor_action,
                before_head_commit = EXCLUDED.before_head_commit,
                after_head_commit = EXCLUDED.after_head_commit,
                status = EXCLUDED.status,
                exit_code = EXCLUDED.exit_code,
                dropped = EXCLUDED.dropped,
                pid = EXCLUDED.pid,
                completed_at = EXCLUDED.completed_at
            RETURNING
                id, attempt_id, node_id, run_reason,
                executor_action, before_head_commit, after_head_commit,
                status, exit_code, dropped, pid,
                started_at, completed_at, created_at
            "#,
        )
        .bind(data.id)
        .bind(data.attempt_id)
        .bind(data.node_id)
        .bind(&data.run_reason)
        .bind(&data.executor_action)
        .bind(&data.before_head_commit)
        .bind(&data.after_head_commit)
        .bind(&data.status)
        .bind(data.exit_code)
        .bind(data.dropped)
        .bind(data.pid)
        .bind(data.started_at)
        .bind(data.completed_at)
        .bind(data.created_at)
        .fetch_one(self.pool)
        .await?;

        Ok(process)
    }

    /// Find a node execution process by ID
    pub async fn find_by_id(
        &self,
        id: Uuid,
    ) -> Result<Option<NodeExecutionProcess>, NodeExecutionProcessError> {
        let process = sqlx::query_as::<_, NodeExecutionProcess>(
            r#"
            SELECT
                id, attempt_id, node_id, run_reason,
                executor_action, before_head_commit, after_head_commit,
                status, exit_code, dropped, pid,
                started_at, completed_at, created_at
            FROM node_execution_processes
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(self.pool)
        .await?;

        Ok(process)
    }

    /// Find all execution processes for an attempt
    pub async fn find_by_attempt_id(
        &self,
        attempt_id: Uuid,
    ) -> Result<Vec<NodeExecutionProcess>, NodeExecutionProcessError> {
        let processes = sqlx::query_as::<_, NodeExecutionProcess>(
            r#"
            SELECT
                id, attempt_id, node_id, run_reason,
                executor_action, before_head_commit, after_head_commit,
                status, exit_code, dropped, pid,
                started_at, completed_at, created_at
            FROM node_execution_processes
            WHERE attempt_id = $1
            ORDER BY created_at ASC
            "#,
        )
        .bind(attempt_id)
        .fetch_all(self.pool)
        .await?;

        Ok(processes)
    }

    /// Find all execution processes for a node
    pub async fn find_by_node_id(
        &self,
        node_id: Uuid,
    ) -> Result<Vec<NodeExecutionProcess>, NodeExecutionProcessError> {
        let processes = sqlx::query_as::<_, NodeExecutionProcess>(
            r#"
            SELECT
                id, attempt_id, node_id, run_reason,
                executor_action, before_head_commit, after_head_commit,
                status, exit_code, dropped, pid,
                started_at, completed_at, created_at
            FROM node_execution_processes
            WHERE node_id = $1
            ORDER BY created_at DESC
            "#,
        )
        .bind(node_id)
        .fetch_all(self.pool)
        .await?;

        Ok(processes)
    }

    /// Find running execution processes for a node
    pub async fn find_running_by_node_id(
        &self,
        node_id: Uuid,
    ) -> Result<Vec<NodeExecutionProcess>, NodeExecutionProcessError> {
        let processes = sqlx::query_as::<_, NodeExecutionProcess>(
            r#"
            SELECT
                id, attempt_id, node_id, run_reason,
                executor_action, before_head_commit, after_head_commit,
                status, exit_code, dropped, pid,
                started_at, completed_at, created_at
            FROM node_execution_processes
            WHERE node_id = $1 AND status = 'running'
            ORDER BY created_at DESC
            "#,
        )
        .bind(node_id)
        .fetch_all(self.pool)
        .await?;

        Ok(processes)
    }

    /// Delete a node execution process
    pub async fn delete(&self, id: Uuid) -> Result<bool, NodeExecutionProcessError> {
        let result = sqlx::query("DELETE FROM node_execution_processes WHERE id = $1")
            .bind(id)
            .execute(self.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }
}
