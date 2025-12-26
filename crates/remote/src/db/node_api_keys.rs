use chrono::Utc;
use sqlx::PgPool;
use thiserror::Error;
use uuid::Uuid;

use crate::nodes::{CreateNodeApiKey, NodeApiKey};

#[derive(Debug, Error)]
pub enum NodeApiKeyError {
    #[error("API key not found")]
    NotFound,
    #[error("API key revoked")]
    Revoked,
    #[error("API key blocked: {0}")]
    Blocked(String),
    #[error("API key already bound to a different node")]
    AlreadyBound,
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}

pub struct NodeApiKeyRepository<'a> {
    pool: &'a PgPool,
}

impl<'a> NodeApiKeyRepository<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    /// Create a new API key. Returns the key record and the raw key value (only available at creation).
    pub async fn create(
        &self,
        organization_id: Uuid,
        data: CreateNodeApiKey,
        created_by: Uuid,
        key_hash: &str,
        key_prefix: &str,
    ) -> Result<NodeApiKey, NodeApiKeyError> {
        let key = sqlx::query_as::<_, NodeApiKey>(
            r#"
            INSERT INTO node_api_keys (organization_id, name, key_hash, key_prefix, created_by)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING
                id,
                organization_id,
                name,
                key_hash,
                key_prefix,
                created_by,
                last_used_at,
                revoked_at,
                created_at,
                node_id,
                takeover_count,
                takeover_window_start,
                blocked_at,
                blocked_reason
            "#,
        )
        .bind(organization_id)
        .bind(&data.name)
        .bind(key_hash)
        .bind(key_prefix)
        .bind(created_by)
        .fetch_one(self.pool)
        .await?;

        Ok(key)
    }

    /// Find an API key by its prefix (first 8 chars) for validation
    pub async fn find_by_prefix(
        &self,
        prefix: &str,
    ) -> Result<Option<NodeApiKey>, NodeApiKeyError> {
        let key = sqlx::query_as::<_, NodeApiKey>(
            r#"
            SELECT
                id,
                organization_id,
                name,
                key_hash,
                key_prefix,
                created_by,
                last_used_at,
                revoked_at,
                created_at,
                node_id,
                takeover_count,
                takeover_window_start,
                blocked_at,
                blocked_reason
            FROM node_api_keys
            WHERE key_prefix = $1
            "#,
        )
        .bind(prefix)
        .fetch_optional(self.pool)
        .await?;

        Ok(key)
    }

    /// List all API keys for an organization
    pub async fn list_by_organization(
        &self,
        organization_id: Uuid,
    ) -> Result<Vec<NodeApiKey>, NodeApiKeyError> {
        let keys = sqlx::query_as::<_, NodeApiKey>(
            r#"
            SELECT
                id,
                organization_id,
                name,
                key_hash,
                key_prefix,
                created_by,
                last_used_at,
                revoked_at,
                created_at,
                node_id,
                takeover_count,
                takeover_window_start,
                blocked_at,
                blocked_reason
            FROM node_api_keys
            WHERE organization_id = $1
            ORDER BY created_at DESC
            "#,
        )
        .bind(organization_id)
        .fetch_all(self.pool)
        .await?;

        Ok(keys)
    }

    /// Update the last_used_at timestamp
    pub async fn touch(&self, key_id: Uuid) -> Result<(), NodeApiKeyError> {
        sqlx::query(
            r#"
            UPDATE node_api_keys
            SET last_used_at = $2
            WHERE id = $1
            "#,
        )
        .bind(key_id)
        .bind(Utc::now())
        .execute(self.pool)
        .await?;

        Ok(())
    }

    /// Revoke an API key
    pub async fn revoke(&self, key_id: Uuid) -> Result<(), NodeApiKeyError> {
        let result = sqlx::query(
            r#"
            UPDATE node_api_keys
            SET revoked_at = $2
            WHERE id = $1 AND revoked_at IS NULL
            "#,
        )
        .bind(key_id)
        .bind(Utc::now())
        .execute(self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(NodeApiKeyError::NotFound);
        }

        Ok(())
    }

    /// Delete an API key permanently
    pub async fn delete(&self, key_id: Uuid) -> Result<(), NodeApiKeyError> {
        let result = sqlx::query(
            r#"
            DELETE FROM node_api_keys
            WHERE id = $1
            "#,
        )
        .bind(key_id)
        .execute(self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(NodeApiKeyError::NotFound);
        }

        Ok(())
    }

    /// Bind an API key to a specific node.
    /// This establishes the "One API Key = One Node" identity.
    /// Once bound, the key can only be used by this node.
    pub async fn bind_to_node(
        &self,
        key_id: Uuid,
        node_id: Uuid,
    ) -> Result<NodeApiKey, NodeApiKeyError> {
        let key = sqlx::query_as::<_, NodeApiKey>(
            r#"
            UPDATE node_api_keys
            SET node_id = $2,
                takeover_count = 0,
                takeover_window_start = NULL
            WHERE id = $1
            AND (node_id IS NULL OR node_id = $2)
            AND revoked_at IS NULL
            AND blocked_at IS NULL
            RETURNING
                id,
                organization_id,
                name,
                key_hash,
                key_prefix,
                created_by,
                last_used_at,
                revoked_at,
                created_at,
                node_id,
                takeover_count,
                takeover_window_start,
                blocked_at,
                blocked_reason
            "#,
        )
        .bind(key_id)
        .bind(node_id)
        .fetch_optional(self.pool)
        .await?;

        key.ok_or(NodeApiKeyError::AlreadyBound)
    }

    /// Increment takeover count for detecting duplicate key usage.
    /// If the window has expired (> 5 minutes), resets the window and count.
    /// Returns the updated key with the new takeover count.
    pub async fn increment_takeover(
        &self,
        key_id: Uuid,
        window_duration_minutes: i64,
    ) -> Result<NodeApiKey, NodeApiKeyError> {
        let now = Utc::now();
        let window_threshold = now - chrono::Duration::minutes(window_duration_minutes);

        let key = sqlx::query_as::<_, NodeApiKey>(
            r#"
            UPDATE node_api_keys
            SET
                takeover_count = CASE
                    WHEN takeover_window_start IS NULL OR takeover_window_start < $2
                    THEN 1
                    ELSE takeover_count + 1
                END,
                takeover_window_start = CASE
                    WHEN takeover_window_start IS NULL OR takeover_window_start < $2
                    THEN $3
                    ELSE takeover_window_start
                END
            WHERE id = $1
            AND revoked_at IS NULL
            RETURNING
                id,
                organization_id,
                name,
                key_hash,
                key_prefix,
                created_by,
                last_used_at,
                revoked_at,
                created_at,
                node_id,
                takeover_count,
                takeover_window_start,
                blocked_at,
                blocked_reason
            "#,
        )
        .bind(key_id)
        .bind(window_threshold)
        .bind(now)
        .fetch_optional(self.pool)
        .await?;

        key.ok_or(NodeApiKeyError::NotFound)
    }

    /// Block an API key due to suspected duplicate use or other security issues.
    /// Blocked keys cannot be used until unblocked by an admin.
    pub async fn block_key(
        &self,
        key_id: Uuid,
        reason: &str,
    ) -> Result<NodeApiKey, NodeApiKeyError> {
        let key = sqlx::query_as::<_, NodeApiKey>(
            r#"
            UPDATE node_api_keys
            SET
                blocked_at = $2,
                blocked_reason = $3
            WHERE id = $1
            AND revoked_at IS NULL
            AND blocked_at IS NULL
            RETURNING
                id,
                organization_id,
                name,
                key_hash,
                key_prefix,
                created_by,
                last_used_at,
                revoked_at,
                created_at,
                node_id,
                takeover_count,
                takeover_window_start,
                blocked_at,
                blocked_reason
            "#,
        )
        .bind(key_id)
        .bind(Utc::now())
        .bind(reason)
        .fetch_optional(self.pool)
        .await?;

        key.ok_or(NodeApiKeyError::NotFound)
    }

    /// Unblock a previously blocked API key.
    /// Resets takeover count and clears the block status.
    pub async fn unblock_key(&self, key_id: Uuid) -> Result<NodeApiKey, NodeApiKeyError> {
        let key = sqlx::query_as::<_, NodeApiKey>(
            r#"
            UPDATE node_api_keys
            SET
                blocked_at = NULL,
                blocked_reason = NULL,
                takeover_count = 0,
                takeover_window_start = NULL
            WHERE id = $1
            AND revoked_at IS NULL
            AND blocked_at IS NOT NULL
            RETURNING
                id,
                organization_id,
                name,
                key_hash,
                key_prefix,
                created_by,
                last_used_at,
                revoked_at,
                created_at,
                node_id,
                takeover_count,
                takeover_window_start,
                blocked_at,
                blocked_reason
            "#,
        )
        .bind(key_id)
        .fetch_optional(self.pool)
        .await?;

        key.ok_or(NodeApiKeyError::NotFound)
    }

    /// Reset takeover count without unblocking.
    /// Used when a legitimate takeover is allowed (node was offline).
    pub async fn reset_takeover_count(&self, key_id: Uuid) -> Result<(), NodeApiKeyError> {
        let result = sqlx::query(
            r#"
            UPDATE node_api_keys
            SET
                takeover_count = 0,
                takeover_window_start = NULL
            WHERE id = $1
            AND revoked_at IS NULL
            "#,
        )
        .bind(key_id)
        .execute(self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(NodeApiKeyError::NotFound);
        }

        Ok(())
    }

    /// Find an API key by ID
    pub async fn find_by_id(&self, key_id: Uuid) -> Result<Option<NodeApiKey>, NodeApiKeyError> {
        let key = sqlx::query_as::<_, NodeApiKey>(
            r#"
            SELECT
                id,
                organization_id,
                name,
                key_hash,
                key_prefix,
                created_by,
                last_used_at,
                revoked_at,
                created_at,
                node_id,
                takeover_count,
                takeover_window_start,
                blocked_at,
                blocked_reason
            FROM node_api_keys
            WHERE id = $1
            "#,
        )
        .bind(key_id)
        .fetch_optional(self.pool)
        .await?;

        Ok(key)
    }

    /// Update the node binding for an API key (used during takeover)
    pub async fn update_node_binding(
        &self,
        key_id: Uuid,
        node_id: Uuid,
        reset_takeover: bool,
    ) -> Result<NodeApiKey, NodeApiKeyError> {
        let key = if reset_takeover {
            sqlx::query_as::<_, NodeApiKey>(
                r#"
                UPDATE node_api_keys
                SET
                    node_id = $2,
                    takeover_count = 0,
                    takeover_window_start = NULL,
                    last_used_at = $3
                WHERE id = $1
                AND revoked_at IS NULL
                AND blocked_at IS NULL
                RETURNING
                    id,
                    organization_id,
                    name,
                    key_hash,
                    key_prefix,
                    created_by,
                    last_used_at,
                    revoked_at,
                    created_at,
                    node_id,
                    takeover_count,
                    takeover_window_start,
                    blocked_at,
                    blocked_reason
                "#,
            )
            .bind(key_id)
            .bind(node_id)
            .bind(Utc::now())
            .fetch_optional(self.pool)
            .await?
        } else {
            sqlx::query_as::<_, NodeApiKey>(
                r#"
                UPDATE node_api_keys
                SET
                    node_id = $2,
                    last_used_at = $3
                WHERE id = $1
                AND revoked_at IS NULL
                AND blocked_at IS NULL
                RETURNING
                    id,
                    organization_id,
                    name,
                    key_hash,
                    key_prefix,
                    created_by,
                    last_used_at,
                    revoked_at,
                    created_at,
                    node_id,
                    takeover_count,
                    takeover_window_start,
                    blocked_at,
                    blocked_reason
                "#,
            )
            .bind(key_id)
            .bind(node_id)
            .bind(Utc::now())
            .fetch_optional(self.pool)
            .await?
        };

        key.ok_or(NodeApiKeyError::NotFound)
    }
}
