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
                created_at, updated_at, sync_state
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, 'partial')
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
                created_at, updated_at, sync_state, sync_requested_at, last_full_sync_at
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
                created_at, updated_at, sync_state, sync_requested_at, last_full_sync_at
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
                created_at, updated_at, sync_state, sync_requested_at, last_full_sync_at
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
                created_at, updated_at, sync_state, sync_requested_at, last_full_sync_at
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

    /// Find incomplete attempts for a specific node (for reconciliation on reconnect)
    pub async fn find_incomplete_for_node(
        &self,
        node_id: Uuid,
    ) -> Result<Vec<NodeTaskAttempt>, NodeTaskAttemptError> {
        let attempts = sqlx::query_as::<_, NodeTaskAttempt>(
            r#"
            SELECT
                id, assignment_id, shared_task_id, node_id,
                executor, executor_variant, branch, target_branch,
                container_ref, worktree_deleted, setup_completed_at,
                created_at, updated_at, sync_state, sync_requested_at, last_full_sync_at
            FROM node_task_attempts
            WHERE node_id = $1 AND sync_state != 'complete'
            ORDER BY created_at DESC
            "#,
        )
        .bind(node_id)
        .fetch_all(self.pool)
        .await?;

        Ok(attempts)
    }

    /// Find all incomplete attempts where the node is currently online
    /// Used by periodic reconciliation
    ///
    /// # Arguments
    /// * `limit` - Maximum number of results to return
    /// * `offset` - Number of results to skip (for pagination)
    pub async fn find_incomplete_with_online_nodes(
        &self,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<NodeTaskAttempt>, NodeTaskAttemptError> {
        let attempts = sqlx::query_as::<_, NodeTaskAttempt>(
            r#"
            SELECT
                nta.id, nta.assignment_id, nta.shared_task_id, nta.node_id,
                nta.executor, nta.executor_variant, nta.branch, nta.target_branch,
                nta.container_ref, nta.worktree_deleted, nta.setup_completed_at,
                nta.created_at, nta.updated_at, nta.sync_state, nta.sync_requested_at, nta.last_full_sync_at
            FROM node_task_attempts nta
            INNER JOIN nodes n ON nta.node_id = n.id
            WHERE nta.sync_state != 'complete'
              AND n.last_seen_at > NOW() - INTERVAL '5 minutes'
            ORDER BY nta.created_at DESC
            LIMIT $1 OFFSET $2
            "#,
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(self.pool)
        .await?;

        Ok(attempts)
    }

    /// Mark attempts as pending backfill
    pub async fn mark_pending_backfill(&self, ids: &[Uuid]) -> Result<u64, NodeTaskAttemptError> {
        if ids.is_empty() {
            return Ok(0);
        }

        let result = sqlx::query(
            r#"
            UPDATE node_task_attempts
            SET sync_state = 'pending_backfill',
                sync_requested_at = NOW()
            WHERE id = ANY($1) AND sync_state = 'partial'
            "#,
        )
        .bind(ids)
        .execute(self.pool)
        .await?;

        Ok(result.rows_affected())
    }

    /// Mark an attempt as complete (fully synced)
    pub async fn mark_complete(&self, id: Uuid) -> Result<bool, NodeTaskAttemptError> {
        let result = sqlx::query(
            r#"
            UPDATE node_task_attempts
            SET sync_state = 'complete',
                last_full_sync_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(id)
        .execute(self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Reset pending backfill attempts that have timed out (node went offline)
    /// Called periodically to reset stale pending_backfill states
    pub async fn reset_stale_pending_backfill(
        &self,
        timeout_minutes: i32,
    ) -> Result<u64, NodeTaskAttemptError> {
        let result = sqlx::query(
            r#"
            UPDATE node_task_attempts
            SET sync_state = 'partial'
            WHERE sync_state = 'pending_backfill'
              AND sync_requested_at < NOW() - make_interval(mins => $1)
            "#,
        )
        .bind(timeout_minutes)
        .execute(self.pool)
        .await?;

        Ok(result.rows_affected())
    }

    /// Reset specific failed backfill attempts to partial state.
    ///
    /// Called when a BackfillResponse indicates failure, allowing the attempts
    /// to be retried on the next periodic check or reconnect.
    pub async fn reset_failed_backfill(&self, node_id: Uuid) -> Result<u64, NodeTaskAttemptError> {
        let result = sqlx::query(
            r#"
            UPDATE node_task_attempts
            SET sync_state = 'partial'
            WHERE node_id = $1 AND sync_state = 'pending_backfill'
            "#,
        )
        .bind(node_id)
        .execute(self.pool)
        .await?;

        Ok(result.rows_affected())
    }

    /// Reset a specific attempt to partial state.
    ///
    /// Used when a backfill request fails or times out for individual attempts.
    /// Only updates if the current state is 'pending_backfill'.
    pub async fn reset_attempt_to_partial(&self, id: Uuid) -> Result<bool, NodeTaskAttemptError> {
        let result = sqlx::query(
            r#"
            UPDATE node_task_attempts
            SET sync_state = 'partial', sync_requested_at = NULL
            WHERE id = $1 AND sync_state = 'pending_backfill'
            "#,
        )
        .bind(id)
        .execute(self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }
}
