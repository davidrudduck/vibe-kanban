//! Repository for node task attempts synced from nodes to the Hive.

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use thiserror::Error;
use uuid::Uuid;

use crate::nodes::NodeTaskAttempt;

#[derive(Debug, Error)]
pub enum NodeTaskAttemptError {
    #[error("node task attempt not found")]
    NotFound,
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}

/// Data for creating or upserting a node task attempt
#[derive(Debug, Clone)]
pub struct UpsertNodeTaskAttempt {
    pub id: Uuid,
    pub assignment_id: Option<Uuid>,
    pub shared_task_id: Uuid,
    pub node_id: Uuid,
    pub executor: String,
    pub executor_variant: Option<String>,
    pub branch: String,
    pub target_branch: String,
    pub container_ref: Option<String>,
    pub worktree_deleted: bool,
    pub setup_completed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub struct NodeTaskAttemptRepository<'a> {
    pool: &'a PgPool,
}

impl<'a> NodeTaskAttemptRepository<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    /// Upsert a node task attempt (insert or update on conflict)
    pub async fn upsert(
        &self,
        data: &UpsertNodeTaskAttempt,
    ) -> Result<NodeTaskAttempt, NodeTaskAttemptError> {
        let attempt = sqlx::query_as::<_, NodeTaskAttempt>(
            r#"
            INSERT INTO node_task_attempts (
                id, assignment_id, shared_task_id, node_id,
                executor, executor_variant, branch, target_branch,
                container_ref, worktree_deleted, setup_completed_at,
                created_at, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
            ON CONFLICT (id) DO UPDATE SET
                assignment_id = EXCLUDED.assignment_id,
                executor = EXCLUDED.executor,
                executor_variant = EXCLUDED.executor_variant,
                branch = EXCLUDED.branch,
                target_branch = EXCLUDED.target_branch,
                container_ref = EXCLUDED.container_ref,
                worktree_deleted = EXCLUDED.worktree_deleted,
                setup_completed_at = EXCLUDED.setup_completed_at,
                updated_at = EXCLUDED.updated_at
            RETURNING
                id, assignment_id, shared_task_id, node_id,
                executor, executor_variant, branch, target_branch,
                container_ref, worktree_deleted, setup_completed_at,
                created_at, updated_at
            "#,
        )
        .bind(data.id)
        .bind(data.assignment_id)
        .bind(data.shared_task_id)
        .bind(data.node_id)
        .bind(&data.executor)
        .bind(&data.executor_variant)
        .bind(&data.branch)
        .bind(&data.target_branch)
        .bind(&data.container_ref)
        .bind(data.worktree_deleted)
        .bind(data.setup_completed_at)
        .bind(data.created_at)
        .bind(data.updated_at)
        .fetch_one(self.pool)
        .await?;

        Ok(attempt)
    }

    /// Find a node task attempt by ID
    pub async fn find_by_id(
        &self,
        id: Uuid,
    ) -> Result<Option<NodeTaskAttempt>, NodeTaskAttemptError> {
        let attempt = sqlx::query_as::<_, NodeTaskAttempt>(
            r#"
            SELECT
                id, assignment_id, shared_task_id, node_id,
                executor, executor_variant, branch, target_branch,
                container_ref, worktree_deleted, setup_completed_at,
                created_at, updated_at
            FROM node_task_attempts
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(self.pool)
        .await?;

        Ok(attempt)
    }

    /// Find all attempts for a shared task
    pub async fn find_by_shared_task_id(
        &self,
        shared_task_id: Uuid,
    ) -> Result<Vec<NodeTaskAttempt>, NodeTaskAttemptError> {
        let attempts = sqlx::query_as::<_, NodeTaskAttempt>(
            r#"
            SELECT
                id, assignment_id, shared_task_id, node_id,
                executor, executor_variant, branch, target_branch,
                container_ref, worktree_deleted, setup_completed_at,
                created_at, updated_at
            FROM node_task_attempts
            WHERE shared_task_id = $1
            ORDER BY created_at DESC
            "#,
        )
        .bind(shared_task_id)
        .fetch_all(self.pool)
        .await?;

        Ok(attempts)
    }

    /// Find all attempts for a node
    pub async fn find_by_node_id(
        &self,
        node_id: Uuid,
    ) -> Result<Vec<NodeTaskAttempt>, NodeTaskAttemptError> {
        let attempts = sqlx::query_as::<_, NodeTaskAttempt>(
            r#"
            SELECT
                id, assignment_id, shared_task_id, node_id,
                executor, executor_variant, branch, target_branch,
                container_ref, worktree_deleted, setup_completed_at,
                created_at, updated_at
            FROM node_task_attempts
            WHERE node_id = $1
            ORDER BY created_at DESC
            "#,
        )
        .bind(node_id)
        .fetch_all(self.pool)
        .await?;

        Ok(attempts)
    }

    /// Delete a node task attempt
    pub async fn delete(&self, id: Uuid) -> Result<bool, NodeTaskAttemptError> {
        let result = sqlx::query("DELETE FROM node_task_attempts WHERE id = $1")
            .bind(id)
            .execute(self.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }
}
