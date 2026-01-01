//! Repository for swarm projects - projects that are shared across the swarm.
//!
//! Swarm projects are distinct from regular projects. They represent explicitly linked
//! projects that can have multiple node projects attached, enabling task sharing across nodes.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgPool;
use thiserror::Error;
use uuid::Uuid;

use super::Tx;

/// A swarm project that can be linked to multiple node projects.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct SwarmProject {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub metadata: Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Data required to create a new swarm project.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSwarmProjectData {
    pub organization_id: Uuid,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub metadata: Value,
}

/// Data for updating an existing swarm project.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateSwarmProjectData {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
}

/// A link between a swarm project and a node's local project.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct SwarmProjectNode {
    pub id: Uuid,
    pub swarm_project_id: Uuid,
    pub node_id: Uuid,
    pub local_project_id: Uuid,
    pub git_repo_path: String,
    pub os_type: Option<String>,
    pub linked_at: DateTime<Utc>,
}

/// Data required to link a node project to a swarm project.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkSwarmProjectNodeData {
    pub swarm_project_id: Uuid,
    pub node_id: Uuid,
    pub local_project_id: Uuid,
    pub git_repo_path: String,
    #[serde(default)]
    pub os_type: Option<String>,
}

/// Extended swarm project info with linked nodes count.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct SwarmProjectWithNodesRow {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub metadata: Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub linked_nodes_count: i64,
}

/// Extended swarm project info with linked nodes count.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmProjectWithNodes {
    #[serde(flatten)]
    pub project: SwarmProject,
    pub linked_nodes_count: i64,
}

impl From<SwarmProjectWithNodesRow> for SwarmProjectWithNodes {
    fn from(row: SwarmProjectWithNodesRow) -> Self {
        Self {
            project: SwarmProject {
                id: row.id,
                organization_id: row.organization_id,
                name: row.name,
                description: row.description,
                metadata: row.metadata,
                created_at: row.created_at,
                updated_at: row.updated_at,
            },
            linked_nodes_count: row.linked_nodes_count,
        }
    }
}

/// Errors that can occur during swarm project operations.
#[derive(Debug, Error)]
pub enum SwarmProjectError {
    #[error("swarm project not found")]
    NotFound,
    #[error("swarm project name already exists in organization")]
    NameConflict,
    #[error("node project link already exists")]
    LinkAlreadyExists,
    #[error("node project link not found")]
    LinkNotFound,
    #[error("invalid metadata: must be a JSON object")]
    InvalidMetadata,
    #[error("cannot merge project with itself")]
    CannotMergeSelf,
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

/// Repository for swarm project operations.
pub struct SwarmProjectRepository;

impl SwarmProjectRepository {
    /// Find a swarm project by ID.
    pub async fn find_by_id(
        pool: &PgPool,
        id: Uuid,
    ) -> Result<Option<SwarmProject>, SwarmProjectError> {
        let record = sqlx::query_as::<_, SwarmProject>(
            r#"
            SELECT
                id,
                organization_id,
                name,
                description,
                metadata,
                created_at,
                updated_at
            FROM swarm_projects
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(pool)
        .await?;

        Ok(record)
    }

    /// Find a swarm project by ID within a transaction.
    pub async fn find_by_id_tx(
        tx: &mut Tx<'_>,
        id: Uuid,
    ) -> Result<Option<SwarmProject>, SwarmProjectError> {
        let record = sqlx::query_as::<_, SwarmProject>(
            r#"
            SELECT
                id,
                organization_id,
                name,
                description,
                metadata,
                created_at,
                updated_at
            FROM swarm_projects
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&mut **tx)
        .await?;

        Ok(record)
    }

    /// List all swarm projects for an organization.
    pub async fn list_by_organization(
        pool: &PgPool,
        organization_id: Uuid,
    ) -> Result<Vec<SwarmProject>, SwarmProjectError> {
        let records = sqlx::query_as::<_, SwarmProject>(
            r#"
            SELECT
                id,
                organization_id,
                name,
                description,
                metadata,
                created_at,
                updated_at
            FROM swarm_projects
            WHERE organization_id = $1
            ORDER BY created_at DESC
            "#,
        )
        .bind(organization_id)
        .fetch_all(pool)
        .await?;

        Ok(records)
    }

    /// List all swarm projects for an organization with linked nodes count.
    pub async fn list_with_nodes_count(
        pool: &PgPool,
        organization_id: Uuid,
    ) -> Result<Vec<SwarmProjectWithNodes>, SwarmProjectError> {
        let records = sqlx::query_as::<_, SwarmProjectWithNodesRow>(
            r#"
            SELECT
                sp.id,
                sp.organization_id,
                sp.name,
                sp.description,
                sp.metadata,
                sp.created_at,
                sp.updated_at,
                COUNT(spn.id)::bigint AS linked_nodes_count
            FROM swarm_projects sp
            LEFT JOIN swarm_project_nodes spn ON sp.id = spn.swarm_project_id
            WHERE sp.organization_id = $1
            GROUP BY sp.id
            ORDER BY sp.created_at DESC
            "#,
        )
        .bind(organization_id)
        .fetch_all(pool)
        .await?;

        Ok(records.into_iter().map(SwarmProjectWithNodes::from).collect())
    }

    /// Create a new swarm project.
    pub async fn create(
        tx: &mut Tx<'_>,
        data: CreateSwarmProjectData,
    ) -> Result<SwarmProject, SwarmProjectError> {
        let metadata = normalize_metadata(data.metadata)?;

        let record = sqlx::query_as::<_, SwarmProject>(
            r#"
            INSERT INTO swarm_projects (organization_id, name, description, metadata)
            VALUES ($1, $2, $3, $4)
            RETURNING
                id,
                organization_id,
                name,
                description,
                metadata,
                created_at,
                updated_at
            "#,
        )
        .bind(data.organization_id)
        .bind(data.name)
        .bind(data.description)
        .bind(metadata)
        .fetch_one(&mut **tx)
        .await
        .map_err(|e| {
            if let sqlx::Error::Database(ref db_err) = e
                && db_err.constraint() == Some("swarm_projects_organization_id_name_key")
            {
                return SwarmProjectError::NameConflict;
            }
            SwarmProjectError::Database(e)
        })?;

        Ok(record)
    }

    /// Update an existing swarm project.
    pub async fn update(
        tx: &mut Tx<'_>,
        id: Uuid,
        data: UpdateSwarmProjectData,
    ) -> Result<SwarmProject, SwarmProjectError> {
        // First check if the project exists
        let existing = Self::find_by_id_tx(tx, id).await?;
        if existing.is_none() {
            return Err(SwarmProjectError::NotFound);
        }

        let metadata = data.metadata.map(normalize_metadata).transpose()?;

        let record = sqlx::query_as::<_, SwarmProject>(
            r#"
            UPDATE swarm_projects
            SET
                name = COALESCE($2, name),
                description = COALESCE($3, description),
                metadata = COALESCE($4, metadata),
                updated_at = NOW()
            WHERE id = $1
            RETURNING
                id,
                organization_id,
                name,
                description,
                metadata,
                created_at,
                updated_at
            "#,
        )
        .bind(id)
        .bind(data.name)
        .bind(data.description)
        .bind(metadata)
        .fetch_one(&mut **tx)
        .await
        .map_err(|e| {
            if let sqlx::Error::Database(ref db_err) = e
                && db_err.constraint() == Some("swarm_projects_organization_id_name_key")
            {
                return SwarmProjectError::NameConflict;
            }
            SwarmProjectError::Database(e)
        })?;

        Ok(record)
    }

    /// Delete a swarm project and all its node links.
    pub async fn delete(tx: &mut Tx<'_>, id: Uuid) -> Result<(), SwarmProjectError> {
        let result = sqlx::query(
            r#"
            DELETE FROM swarm_projects
            WHERE id = $1
            "#,
        )
        .bind(id)
        .execute(&mut **tx)
        .await?;

        if result.rows_affected() == 0 {
            return Err(SwarmProjectError::NotFound);
        }

        Ok(())
    }

    /// Merge two swarm projects by moving all node links from source to target.
    ///
    /// This operation:
    /// 1. Moves all node links from source_id to target_id
    /// 2. Deletes the source swarm project
    ///
    /// Returns the updated target project.
    pub async fn merge(
        tx: &mut Tx<'_>,
        source_id: Uuid,
        target_id: Uuid,
    ) -> Result<SwarmProject, SwarmProjectError> {
        if source_id == target_id {
            return Err(SwarmProjectError::CannotMergeSelf);
        }

        // Verify both projects exist
        let source = Self::find_by_id_tx(tx, source_id).await?;
        if source.is_none() {
            return Err(SwarmProjectError::NotFound);
        }

        let target = Self::find_by_id_tx(tx, target_id).await?;
        if target.is_none() {
            return Err(SwarmProjectError::NotFound);
        }

        // Move all node links from source to target
        // If a node already has a link to the target, the source link is deleted (via ON CONFLICT)
        sqlx::query(
            r#"
            UPDATE swarm_project_nodes
            SET swarm_project_id = $2
            WHERE swarm_project_id = $1
            AND node_id NOT IN (
                SELECT node_id FROM swarm_project_nodes WHERE swarm_project_id = $2
            )
            "#,
        )
        .bind(source_id)
        .bind(target_id)
        .execute(&mut **tx)
        .await?;

        // Delete remaining source links (nodes that already linked to target)
        sqlx::query(
            r#"
            DELETE FROM swarm_project_nodes
            WHERE swarm_project_id = $1
            "#,
        )
        .bind(source_id)
        .execute(&mut **tx)
        .await?;

        // Delete the source project
        sqlx::query(
            r#"
            DELETE FROM swarm_projects
            WHERE id = $1
            "#,
        )
        .bind(source_id)
        .execute(&mut **tx)
        .await?;

        // Return the updated target project
        Self::find_by_id_tx(tx, target_id)
            .await?
            .ok_or(SwarmProjectError::NotFound)
    }

    // =====================
    // Node Link Operations
    // =====================

    /// Link a node project to a swarm project.
    pub async fn link_node(
        tx: &mut Tx<'_>,
        data: LinkSwarmProjectNodeData,
    ) -> Result<SwarmProjectNode, SwarmProjectError> {
        let record = sqlx::query_as::<_, SwarmProjectNode>(
            r#"
            INSERT INTO swarm_project_nodes (
                swarm_project_id,
                node_id,
                local_project_id,
                git_repo_path,
                os_type
            )
            VALUES ($1, $2, $3, $4, $5)
            RETURNING
                id,
                swarm_project_id,
                node_id,
                local_project_id,
                git_repo_path,
                os_type,
                linked_at
            "#,
        )
        .bind(data.swarm_project_id)
        .bind(data.node_id)
        .bind(data.local_project_id)
        .bind(data.git_repo_path)
        .bind(data.os_type)
        .fetch_one(&mut **tx)
        .await
        .map_err(|e| {
            if let sqlx::Error::Database(ref db_err) = e {
                let constraint = db_err.constraint();
                if constraint == Some("swarm_project_nodes_swarm_project_id_node_id_key")
                    || constraint == Some("swarm_project_nodes_node_id_local_project_id_key")
                {
                    return SwarmProjectError::LinkAlreadyExists;
                }
            }
            SwarmProjectError::Database(e)
        })?;

        Ok(record)
    }

    /// Unlink a node from a swarm project.
    pub async fn unlink_node(
        tx: &mut Tx<'_>,
        swarm_project_id: Uuid,
        node_id: Uuid,
    ) -> Result<(), SwarmProjectError> {
        let result = sqlx::query(
            r#"
            DELETE FROM swarm_project_nodes
            WHERE swarm_project_id = $1 AND node_id = $2
            "#,
        )
        .bind(swarm_project_id)
        .bind(node_id)
        .execute(&mut **tx)
        .await?;

        if result.rows_affected() == 0 {
            return Err(SwarmProjectError::LinkNotFound);
        }

        Ok(())
    }

    /// List all node links for a swarm project.
    pub async fn list_nodes(
        pool: &PgPool,
        swarm_project_id: Uuid,
    ) -> Result<Vec<SwarmProjectNode>, SwarmProjectError> {
        let records = sqlx::query_as::<_, SwarmProjectNode>(
            r#"
            SELECT
                id,
                swarm_project_id,
                node_id,
                local_project_id,
                git_repo_path,
                os_type,
                linked_at
            FROM swarm_project_nodes
            WHERE swarm_project_id = $1
            ORDER BY linked_at ASC
            "#,
        )
        .bind(swarm_project_id)
        .fetch_all(pool)
        .await?;

        Ok(records)
    }

    /// Find a node link by swarm project and node.
    pub async fn find_node_link(
        pool: &PgPool,
        swarm_project_id: Uuid,
        node_id: Uuid,
    ) -> Result<Option<SwarmProjectNode>, SwarmProjectError> {
        let record = sqlx::query_as::<_, SwarmProjectNode>(
            r#"
            SELECT
                id,
                swarm_project_id,
                node_id,
                local_project_id,
                git_repo_path,
                os_type,
                linked_at
            FROM swarm_project_nodes
            WHERE swarm_project_id = $1 AND node_id = $2
            "#,
        )
        .bind(swarm_project_id)
        .bind(node_id)
        .fetch_optional(pool)
        .await?;

        Ok(record)
    }

    /// List all swarm project links for a node.
    pub async fn list_by_node(
        pool: &PgPool,
        node_id: Uuid,
    ) -> Result<Vec<SwarmProjectNode>, SwarmProjectError> {
        let records = sqlx::query_as::<_, SwarmProjectNode>(
            r#"
            SELECT
                id,
                swarm_project_id,
                node_id,
                local_project_id,
                git_repo_path,
                os_type,
                linked_at
            FROM swarm_project_nodes
            WHERE node_id = $1
            ORDER BY linked_at DESC
            "#,
        )
        .bind(node_id)
        .fetch_all(pool)
        .await?;

        Ok(records)
    }

    /// Get the organization ID for a swarm project.
    pub async fn organization_id(
        pool: &PgPool,
        swarm_project_id: Uuid,
    ) -> Result<Option<Uuid>, SwarmProjectError> {
        let result = sqlx::query_scalar::<_, Uuid>(
            r#"
            SELECT organization_id
            FROM swarm_projects
            WHERE id = $1
            "#,
        )
        .bind(swarm_project_id)
        .fetch_optional(pool)
        .await?;

        Ok(result)
    }
}

/// Normalize metadata to ensure it's a valid JSON object.
fn normalize_metadata(value: Value) -> Result<Value, SwarmProjectError> {
    match value {
        Value::Null => Ok(Value::Object(serde_json::Map::new())),
        Value::Object(_) => Ok(value),
        _ => Err(SwarmProjectError::InvalidMetadata),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_metadata_null() {
        let result = normalize_metadata(Value::Null).unwrap();
        assert!(result.is_object());
    }

    #[test]
    fn test_normalize_metadata_object() {
        let obj = serde_json::json!({"key": "value"});
        let result = normalize_metadata(obj.clone()).unwrap();
        assert_eq!(result, obj);
    }

    #[test]
    fn test_normalize_metadata_array_fails() {
        let arr = serde_json::json!([1, 2, 3]);
        let result = normalize_metadata(arr);
        assert!(matches!(result, Err(SwarmProjectError::InvalidMetadata)));
    }

    #[test]
    fn test_normalize_metadata_string_fails() {
        let s = serde_json::json!("string");
        let result = normalize_metadata(s);
        assert!(matches!(result, Err(SwarmProjectError::InvalidMetadata)));
    }
}
