//! Repository for node local projects - synced from nodes to enable swarm linking.
//!
//! This table tracks ALL local projects that each node reports, allowing users
//! to see and link any project from any node to a swarm project.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use thiserror::Error;
use uuid::Uuid;

use super::Tx;

/// A local project synced from a node.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct NodeLocalProject {
    pub id: Uuid,
    pub node_id: Uuid,
    pub local_project_id: Uuid,
    pub name: String,
    pub git_repo_path: String,
    pub default_branch: String,
    pub swarm_project_id: Option<Uuid>,
    pub last_seen_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

/// Data for upserting a local project from a node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpsertLocalProjectData {
    pub node_id: Uuid,
    pub local_project_id: Uuid,
    pub name: String,
    pub git_repo_path: String,
    pub default_branch: String,
}

/// Errors that can occur during node local project operations.
#[derive(Debug, Error)]
pub enum NodeLocalProjectError {
    #[error("node local project not found")]
    NotFound,
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

/// Repository for node local project operations.
pub struct NodeLocalProjectRepository;

impl NodeLocalProjectRepository {
    /// Upsert a local project from a node sync.
    ///
    /// If the project already exists, updates name, path, branch, and last_seen_at.
    /// If not, creates a new record.
    pub async fn upsert(
        pool: &PgPool,
        data: UpsertLocalProjectData,
    ) -> Result<NodeLocalProject, NodeLocalProjectError> {
        let record = sqlx::query_as::<_, NodeLocalProject>(
            r#"
            INSERT INTO node_local_projects (
                node_id,
                local_project_id,
                name,
                git_repo_path,
                default_branch,
                last_seen_at
            )
            VALUES ($1, $2, $3, $4, $5, NOW())
            ON CONFLICT (node_id, local_project_id)
            DO UPDATE SET
                name = EXCLUDED.name,
                git_repo_path = EXCLUDED.git_repo_path,
                default_branch = EXCLUDED.default_branch,
                last_seen_at = NOW()
            RETURNING *
            "#,
        )
        .bind(data.node_id)
        .bind(data.local_project_id)
        .bind(data.name)
        .bind(data.git_repo_path)
        .bind(data.default_branch)
        .fetch_one(pool)
        .await?;

        Ok(record)
    }

    /// Bulk upsert local projects from a node sync.
    ///
    /// Returns the number of projects upserted.
    pub async fn bulk_upsert(
        pool: &PgPool,
        node_id: Uuid,
        projects: Vec<UpsertLocalProjectData>,
    ) -> Result<usize, NodeLocalProjectError> {
        if projects.is_empty() {
            return Ok(0);
        }

        let mut count = 0;
        for project in projects {
            sqlx::query(
                r#"
                INSERT INTO node_local_projects (
                    node_id,
                    local_project_id,
                    name,
                    git_repo_path,
                    default_branch,
                    last_seen_at
                )
                VALUES ($1, $2, $3, $4, $5, NOW())
                ON CONFLICT (node_id, local_project_id)
                DO UPDATE SET
                    name = EXCLUDED.name,
                    git_repo_path = EXCLUDED.git_repo_path,
                    default_branch = EXCLUDED.default_branch,
                    last_seen_at = NOW()
                "#,
            )
            .bind(node_id)
            .bind(project.local_project_id)
            .bind(project.name)
            .bind(project.git_repo_path)
            .bind(project.default_branch)
            .execute(pool)
            .await?;
            count += 1;
        }

        Ok(count)
    }

    /// List all local projects for a node.
    pub async fn list_by_node(
        pool: &PgPool,
        node_id: Uuid,
    ) -> Result<Vec<NodeLocalProject>, NodeLocalProjectError> {
        let records = sqlx::query_as::<_, NodeLocalProject>(
            r#"
            SELECT *
            FROM node_local_projects
            WHERE node_id = $1
            ORDER BY name ASC
            "#,
        )
        .bind(node_id)
        .fetch_all(pool)
        .await?;

        Ok(records)
    }

    /// List all local projects for a node with swarm project info.
    ///
    /// Includes the swarm project name if the project is linked to one.
    pub async fn list_by_node_with_swarm_info(
        pool: &PgPool,
        node_id: Uuid,
    ) -> Result<Vec<crate::nodes::NodeLocalProjectInfo>, NodeLocalProjectError> {
        let records = sqlx::query_as::<_, crate::nodes::NodeLocalProjectInfo>(
            r#"
            SELECT
                nlp.id,
                nlp.node_id,
                nlp.local_project_id,
                nlp.name,
                nlp.git_repo_path,
                nlp.default_branch,
                nlp.swarm_project_id,
                sp.name as swarm_project_name,
                nlp.last_seen_at,
                nlp.created_at
            FROM node_local_projects nlp
            LEFT JOIN swarm_projects sp ON nlp.swarm_project_id = sp.id
            WHERE nlp.node_id = $1
            ORDER BY nlp.name ASC
            "#,
        )
        .bind(node_id)
        .fetch_all(pool)
        .await?;

        Ok(records)
    }

    /// Find a local project by node and local project ID.
    pub async fn find_by_node_and_project(
        pool: &PgPool,
        node_id: Uuid,
        local_project_id: Uuid,
    ) -> Result<Option<NodeLocalProject>, NodeLocalProjectError> {
        let record = sqlx::query_as::<_, NodeLocalProject>(
            r#"
            SELECT *
            FROM node_local_projects
            WHERE node_id = $1 AND local_project_id = $2
            "#,
        )
        .bind(node_id)
        .bind(local_project_id)
        .fetch_optional(pool)
        .await?;

        Ok(record)
    }

    /// Link a local project to a swarm project.
    pub async fn link_to_swarm(
        tx: &mut Tx<'_>,
        node_id: Uuid,
        local_project_id: Uuid,
        swarm_project_id: Uuid,
    ) -> Result<(), NodeLocalProjectError> {
        let result = sqlx::query(
            r#"
            UPDATE node_local_projects
            SET swarm_project_id = $3
            WHERE node_id = $1 AND local_project_id = $2
            "#,
        )
        .bind(node_id)
        .bind(local_project_id)
        .bind(swarm_project_id)
        .execute(&mut **tx)
        .await?;

        if result.rows_affected() == 0 {
            return Err(NodeLocalProjectError::NotFound);
        }

        Ok(())
    }

    /// Unlink a local project from its swarm project.
    pub async fn unlink_from_swarm(
        tx: &mut Tx<'_>,
        node_id: Uuid,
        local_project_id: Uuid,
    ) -> Result<(), NodeLocalProjectError> {
        let result = sqlx::query(
            r#"
            UPDATE node_local_projects
            SET swarm_project_id = NULL
            WHERE node_id = $1 AND local_project_id = $2
            "#,
        )
        .bind(node_id)
        .bind(local_project_id)
        .execute(&mut **tx)
        .await?;

        if result.rows_affected() == 0 {
            return Err(NodeLocalProjectError::NotFound);
        }

        Ok(())
    }

    /// Delete stale projects that haven't been seen in the specified duration.
    ///
    /// Only deletes projects from online nodes (if the node is offline, the project
    /// might just be waiting for the node to reconnect).
    pub async fn delete_stale(
        pool: &PgPool,
        stale_threshold: chrono::Duration,
    ) -> Result<u64, NodeLocalProjectError> {
        let threshold = Utc::now() - stale_threshold;

        let result = sqlx::query(
            r#"
            DELETE FROM node_local_projects nlp
            USING nodes n
            WHERE nlp.node_id = n.id
              AND n.status = 'online'
              AND nlp.last_seen_at < $1
            "#,
        )
        .bind(threshold)
        .execute(pool)
        .await?;

        Ok(result.rows_affected())
    }

    /// Update last_seen_at for all projects from a node.
    ///
    /// Called during heartbeat to keep projects fresh.
    pub async fn touch_all_for_node(
        pool: &PgPool,
        node_id: Uuid,
    ) -> Result<u64, NodeLocalProjectError> {
        let result = sqlx::query(
            r#"
            UPDATE node_local_projects
            SET last_seen_at = NOW()
            WHERE node_id = $1
            "#,
        )
        .bind(node_id)
        .execute(pool)
        .await?;

        Ok(result.rows_affected())
    }

    /// Delete all local projects for a node.
    ///
    /// Used when a node is deleted or during cleanup.
    pub async fn delete_by_node(
        pool: &PgPool,
        node_id: Uuid,
    ) -> Result<u64, NodeLocalProjectError> {
        let result = sqlx::query(
            r#"
            DELETE FROM node_local_projects
            WHERE node_id = $1
            "#,
        )
        .bind(node_id)
        .execute(pool)
        .await?;

        Ok(result.rows_affected())
    }
}
