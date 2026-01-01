//! Repository for swarm templates - organization-wide task templates.
//!
//! Swarm templates are org-global templates that can be used across all nodes
//! in the swarm. They provide reusable task descriptions and configurations.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgPool;
use thiserror::Error;
use uuid::Uuid;

use super::Tx;

/// A swarm template for task descriptions.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct SwarmTemplate {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub name: String,
    pub content: String,
    pub description: Option<String>,
    pub metadata: Value,
    pub version: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}

/// Data required to create a new swarm template.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSwarmTemplateData {
    pub organization_id: Uuid,
    pub name: String,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub metadata: Value,
}

/// Data for updating an existing swarm template.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateSwarmTemplateData {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<i64>,
}

/// Errors that can occur during swarm template operations.
#[derive(Debug, Error)]
pub enum SwarmTemplateError {
    #[error("swarm template not found")]
    NotFound,
    #[error("swarm template name already exists in organization")]
    NameConflict,
    #[error("version mismatch")]
    VersionMismatch,
    #[error("invalid metadata: must be a JSON object")]
    InvalidMetadata,
    #[error("cannot merge template with itself")]
    CannotMergeSelf,
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

/// Repository for swarm template operations.
pub struct SwarmTemplateRepository;

impl SwarmTemplateRepository {
    /// Find a swarm template by ID (excludes deleted).
    pub async fn find_by_id(
        pool: &PgPool,
        id: Uuid,
    ) -> Result<Option<SwarmTemplate>, SwarmTemplateError> {
        let record = sqlx::query_as::<_, SwarmTemplate>(
            r#"
            SELECT
                id,
                organization_id,
                name,
                content,
                description,
                metadata,
                version,
                created_at,
                updated_at,
                deleted_at
            FROM swarm_templates
            WHERE id = $1
              AND deleted_at IS NULL
            "#,
        )
        .bind(id)
        .fetch_optional(pool)
        .await?;

        Ok(record)
    }

    /// Find a swarm template by ID within a transaction.
    pub async fn find_by_id_tx(
        tx: &mut Tx<'_>,
        id: Uuid,
    ) -> Result<Option<SwarmTemplate>, SwarmTemplateError> {
        let record = sqlx::query_as::<_, SwarmTemplate>(
            r#"
            SELECT
                id,
                organization_id,
                name,
                content,
                description,
                metadata,
                version,
                created_at,
                updated_at,
                deleted_at
            FROM swarm_templates
            WHERE id = $1
              AND deleted_at IS NULL
            "#,
        )
        .bind(id)
        .fetch_optional(&mut **tx)
        .await?;

        Ok(record)
    }

    /// List all swarm templates for an organization (excludes deleted).
    pub async fn list_by_organization(
        pool: &PgPool,
        organization_id: Uuid,
    ) -> Result<Vec<SwarmTemplate>, SwarmTemplateError> {
        let records = sqlx::query_as::<_, SwarmTemplate>(
            r#"
            SELECT
                id,
                organization_id,
                name,
                content,
                description,
                metadata,
                version,
                created_at,
                updated_at,
                deleted_at
            FROM swarm_templates
            WHERE organization_id = $1
              AND deleted_at IS NULL
            ORDER BY name ASC
            "#,
        )
        .bind(organization_id)
        .fetch_all(pool)
        .await?;

        Ok(records)
    }

    /// Create a new swarm template.
    pub async fn create(
        tx: &mut Tx<'_>,
        data: CreateSwarmTemplateData,
    ) -> Result<SwarmTemplate, SwarmTemplateError> {
        let metadata = normalize_metadata(data.metadata)?;

        let record = sqlx::query_as::<_, SwarmTemplate>(
            r#"
            INSERT INTO swarm_templates (organization_id, name, content, description, metadata)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING
                id,
                organization_id,
                name,
                content,
                description,
                metadata,
                version,
                created_at,
                updated_at,
                deleted_at
            "#,
        )
        .bind(data.organization_id)
        .bind(data.name)
        .bind(data.content)
        .bind(data.description)
        .bind(metadata)
        .fetch_one(&mut **tx)
        .await
        .map_err(|e| {
            if let sqlx::Error::Database(ref db_err) = e
                && db_err.constraint()
                    == Some("swarm_templates_organization_id_name_deleted_at_key")
            {
                return SwarmTemplateError::NameConflict;
            }
            SwarmTemplateError::Database(e)
        })?;

        Ok(record)
    }

    /// Update an existing swarm template with optimistic locking.
    pub async fn update(
        tx: &mut Tx<'_>,
        id: Uuid,
        data: UpdateSwarmTemplateData,
    ) -> Result<SwarmTemplate, SwarmTemplateError> {
        let metadata = data.metadata.map(normalize_metadata).transpose()?;

        let record = sqlx::query_as::<_, SwarmTemplate>(
            r#"
            UPDATE swarm_templates
            SET
                name = COALESCE($2, name),
                content = COALESCE($3, content),
                description = COALESCE($4, description),
                metadata = COALESCE($5, metadata),
                version = version + 1,
                updated_at = NOW()
            WHERE id = $1
              AND version = COALESCE($6, version)
              AND deleted_at IS NULL
            RETURNING
                id,
                organization_id,
                name,
                content,
                description,
                metadata,
                version,
                created_at,
                updated_at,
                deleted_at
            "#,
        )
        .bind(id)
        .bind(data.name)
        .bind(data.content)
        .bind(data.description)
        .bind(metadata)
        .bind(data.version)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or(SwarmTemplateError::VersionMismatch)?;

        Ok(record)
    }

    /// Soft-delete a swarm template.
    pub async fn delete(
        tx: &mut Tx<'_>,
        id: Uuid,
        version: Option<i64>,
    ) -> Result<SwarmTemplate, SwarmTemplateError> {
        let record = sqlx::query_as::<_, SwarmTemplate>(
            r#"
            UPDATE swarm_templates
            SET
                deleted_at = NOW(),
                version = version + 1,
                updated_at = NOW()
            WHERE id = $1
              AND version = COALESCE($2, version)
              AND deleted_at IS NULL
            RETURNING
                id,
                organization_id,
                name,
                content,
                description,
                metadata,
                version,
                created_at,
                updated_at,
                deleted_at
            "#,
        )
        .bind(id)
        .bind(version)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or(SwarmTemplateError::NotFound)?;

        Ok(record)
    }

    /// Merge two swarm templates.
    ///
    /// This operation:
    /// 1. Soft-deletes the source template
    /// 2. Returns the target template
    ///
    /// Note: Unlike labels, templates don't have task associations to migrate.
    /// The merge is primarily for consolidating duplicate templates.
    pub async fn merge(
        tx: &mut Tx<'_>,
        source_id: Uuid,
        target_id: Uuid,
    ) -> Result<SwarmTemplate, SwarmTemplateError> {
        if source_id == target_id {
            return Err(SwarmTemplateError::CannotMergeSelf);
        }

        // Verify both templates exist
        let source = Self::find_by_id_tx(tx, source_id).await?;
        if source.is_none() {
            return Err(SwarmTemplateError::NotFound);
        }

        let target = Self::find_by_id_tx(tx, target_id).await?;
        if target.is_none() {
            return Err(SwarmTemplateError::NotFound);
        }

        // Soft-delete the source template
        sqlx::query(
            r#"
            UPDATE swarm_templates
            SET
                deleted_at = NOW(),
                version = version + 1,
                updated_at = NOW()
            WHERE id = $1
              AND deleted_at IS NULL
            "#,
        )
        .bind(source_id)
        .execute(&mut **tx)
        .await?;

        // Return the target template
        Self::find_by_id_tx(tx, target_id)
            .await?
            .ok_or(SwarmTemplateError::NotFound)
    }

    /// Get the organization ID for a swarm template.
    pub async fn organization_id(
        pool: &PgPool,
        template_id: Uuid,
    ) -> Result<Option<Uuid>, SwarmTemplateError> {
        let result = sqlx::query_scalar::<_, Uuid>(
            r#"
            SELECT organization_id
            FROM swarm_templates
            WHERE id = $1
              AND deleted_at IS NULL
            "#,
        )
        .bind(template_id)
        .fetch_optional(pool)
        .await?;

        Ok(result)
    }
}

/// Normalize metadata to ensure it's a valid JSON object.
fn normalize_metadata(value: Value) -> Result<Value, SwarmTemplateError> {
    match value {
        Value::Null => Ok(Value::Object(serde_json::Map::new())),
        Value::Object(_) => Ok(value),
        _ => Err(SwarmTemplateError::InvalidMetadata),
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
        assert!(matches!(result, Err(SwarmTemplateError::InvalidMetadata)));
    }

    #[test]
    fn test_normalize_metadata_string_fails() {
        let s = serde_json::json!("string");
        let result = normalize_metadata(s);
        assert!(matches!(result, Err(SwarmTemplateError::InvalidMetadata)));
    }
}
