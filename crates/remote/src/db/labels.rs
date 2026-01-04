use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use thiserror::Error;
use uuid::Uuid;

/// A label for visual task categorization in the Hive
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Label {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub project_id: Option<Uuid>,
    pub origin_node_id: Option<Uuid>,
    pub name: String,
    pub icon: String,
    pub color: String,
    pub version: i64,
    pub deleted_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Data for creating a new label
#[derive(Debug, Clone, Deserialize)]
pub struct CreateLabelData {
    pub organization_id: Uuid,
    pub project_id: Option<Uuid>,
    pub origin_node_id: Option<Uuid>,
    pub name: String,
    #[serde(default = "default_icon")]
    pub icon: String,
    #[serde(default = "default_color")]
    pub color: String,
}

fn default_icon() -> String {
    "tag".to_string()
}

fn default_color() -> String {
    "#6b7280".to_string()
}

/// Data for updating an existing label
#[derive(Debug, Clone, Deserialize)]
pub struct UpdateLabelData {
    pub name: Option<String>,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub version: Option<i64>,
}

/// Data for setting labels on a shared task
#[derive(Debug, Clone, Deserialize)]
pub struct SetTaskLabelsData {
    pub label_ids: Vec<Uuid>,
}

/// Payload for label activity events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LabelActivityPayload {
    pub label: Label,
}

#[derive(Debug, Error)]
pub enum LabelError {
    #[error("label not found")]
    NotFound,
    #[error("label conflict: {0}")]
    Conflict(String),
    #[error("version mismatch")]
    VersionMismatch,
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

pub struct LabelRepository<'a> {
    pool: &'a PgPool,
}

impl<'a> LabelRepository<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    /// Find swarm labels for an organization (project_id = NULL, excludes deleted)
    pub async fn find_swarm_labels(
        &self,
        organization_id: Uuid,
    ) -> Result<Vec<Label>, LabelError> {
        let labels = sqlx::query_as::<_, Label>(
            r#"
            SELECT
                id,
                organization_id,
                project_id,
                origin_node_id,
                name,
                icon,
                color,
                version,
                deleted_at,
                created_at,
                updated_at
            FROM labels
            WHERE organization_id = $1
              AND project_id IS NULL
              AND deleted_at IS NULL
            ORDER BY name ASC
            "#,
        )
        .bind(organization_id)
        .fetch_all(self.pool)
        .await?;

        Ok(labels)
    }

    /// Find a label by ID (excludes deleted)
    pub async fn find_by_id(&self, id: Uuid) -> Result<Option<Label>, LabelError> {
        let label = sqlx::query_as!(
            Label,
            r#"
            SELECT
                id              AS "id!",
                organization_id AS "organization_id!",
                project_id      AS "project_id?",
                origin_node_id  AS "origin_node_id?",
                name            AS "name!",
                icon            AS "icon!",
                color           AS "color!",
                version         AS "version!",
                deleted_at      AS "deleted_at?",
                created_at      AS "created_at!",
                updated_at      AS "updated_at!"
            FROM labels
            WHERE id = $1
              AND deleted_at IS NULL
            "#,
            id
        )
        .fetch_optional(self.pool)
        .await?;

        Ok(label)
    }

    /// Find all labels for an organization (excludes deleted)
    pub async fn find_by_organization(
        &self,
        organization_id: Uuid,
    ) -> Result<Vec<Label>, LabelError> {
        let labels = sqlx::query_as!(
            Label,
            r#"
            SELECT
                id              AS "id!",
                organization_id AS "organization_id!",
                project_id      AS "project_id?",
                origin_node_id  AS "origin_node_id?",
                name            AS "name!",
                icon            AS "icon!",
                color           AS "color!",
                version         AS "version!",
                deleted_at      AS "deleted_at?",
                created_at      AS "created_at!",
                updated_at      AS "updated_at!"
            FROM labels
            WHERE organization_id = $1
              AND deleted_at IS NULL
            ORDER BY name ASC
            "#,
            organization_id
        )
        .fetch_all(self.pool)
        .await?;

        Ok(labels)
    }

    /// Find all labels for a project (includes org-global labels where project_id IS NULL)
    pub async fn find_for_project(
        &self,
        organization_id: Uuid,
        project_id: Uuid,
    ) -> Result<Vec<Label>, LabelError> {
        let labels = sqlx::query_as!(
            Label,
            r#"
            SELECT
                id              AS "id!",
                organization_id AS "organization_id!",
                project_id      AS "project_id?",
                origin_node_id  AS "origin_node_id?",
                name            AS "name!",
                icon            AS "icon!",
                color           AS "color!",
                version         AS "version!",
                deleted_at      AS "deleted_at?",
                created_at      AS "created_at!",
                updated_at      AS "updated_at!"
            FROM labels
            WHERE organization_id = $1
              AND (project_id IS NULL OR project_id = $2)
              AND deleted_at IS NULL
            ORDER BY name ASC
            "#,
            organization_id,
            project_id
        )
        .fetch_all(self.pool)
        .await?;

        Ok(labels)
    }

    /// Create a new label
    pub async fn create(&self, data: CreateLabelData) -> Result<Label, LabelError> {
        let label = sqlx::query_as!(
            Label,
            r#"
            INSERT INTO labels (
                organization_id,
                project_id,
                origin_node_id,
                name,
                icon,
                color
            )
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING
                id              AS "id!",
                organization_id AS "organization_id!",
                project_id      AS "project_id?",
                origin_node_id  AS "origin_node_id?",
                name            AS "name!",
                icon            AS "icon!",
                color           AS "color!",
                version         AS "version!",
                deleted_at      AS "deleted_at?",
                created_at      AS "created_at!",
                updated_at      AS "updated_at!"
            "#,
            data.organization_id,
            data.project_id,
            data.origin_node_id,
            data.name,
            data.icon,
            data.color
        )
        .fetch_one(self.pool)
        .await
        .map_err(|e| {
            if let sqlx::Error::Database(ref db_err) = e
                && db_err.constraint()
                    == Some("labels_organization_id_project_id_name_deleted_at_key")
            {
                return LabelError::Conflict(format!(
                    "Label with name '{}' already exists in this scope",
                    data.name
                ));
            }
            LabelError::from(e)
        })?;

        Ok(label)
    }

    /// Update an existing label with optimistic locking
    pub async fn update(&self, id: Uuid, data: UpdateLabelData) -> Result<Label, LabelError> {
        let label = sqlx::query_as!(
            Label,
            r#"
            UPDATE labels AS l
            SET name       = COALESCE($2, l.name),
                icon       = COALESCE($3, l.icon),
                color      = COALESCE($4, l.color),
                version    = l.version + 1,
                updated_at = NOW()
            WHERE l.id = $1
              AND l.version = COALESCE($5, l.version)
              AND l.deleted_at IS NULL
            RETURNING
                l.id              AS "id!",
                l.organization_id AS "organization_id!",
                l.project_id      AS "project_id?",
                l.origin_node_id  AS "origin_node_id?",
                l.name            AS "name!",
                l.icon            AS "icon!",
                l.color           AS "color!",
                l.version         AS "version!",
                l.deleted_at      AS "deleted_at?",
                l.created_at      AS "created_at!",
                l.updated_at      AS "updated_at!"
            "#,
            id,
            data.name,
            data.icon,
            data.color,
            data.version
        )
        .fetch_optional(self.pool)
        .await?
        .ok_or(LabelError::VersionMismatch)?;

        Ok(label)
    }

    /// Soft-delete a label
    pub async fn delete(&self, id: Uuid, version: Option<i64>) -> Result<Label, LabelError> {
        let label = sqlx::query_as!(
            Label,
            r#"
            UPDATE labels AS l
            SET deleted_at = NOW(),
                version    = l.version + 1,
                updated_at = NOW()
            WHERE l.id = $1
              AND l.version = COALESCE($2, l.version)
              AND l.deleted_at IS NULL
            RETURNING
                l.id              AS "id!",
                l.organization_id AS "organization_id!",
                l.project_id      AS "project_id?",
                l.origin_node_id  AS "origin_node_id?",
                l.name            AS "name!",
                l.icon            AS "icon!",
                l.color           AS "color!",
                l.version         AS "version!",
                l.deleted_at      AS "deleted_at?",
                l.created_at      AS "created_at!",
                l.updated_at      AS "updated_at!"
            "#,
            id,
            version
        )
        .fetch_optional(self.pool)
        .await?
        .ok_or(LabelError::NotFound)?;

        Ok(label)
    }

    /// Get all labels for a shared task
    pub async fn find_by_task(&self, shared_task_id: Uuid) -> Result<Vec<Label>, LabelError> {
        let labels = sqlx::query_as!(
            Label,
            r#"
            SELECT
                l.id              AS "id!",
                l.organization_id AS "organization_id!",
                l.project_id      AS "project_id?",
                l.origin_node_id  AS "origin_node_id?",
                l.name            AS "name!",
                l.icon            AS "icon!",
                l.color           AS "color!",
                l.version         AS "version!",
                l.deleted_at      AS "deleted_at?",
                l.created_at      AS "created_at!",
                l.updated_at      AS "updated_at!"
            FROM labels l
            INNER JOIN shared_task_labels stl ON l.id = stl.label_id
            WHERE stl.shared_task_id = $1
              AND l.deleted_at IS NULL
            ORDER BY l.name ASC
            "#,
            shared_task_id
        )
        .fetch_all(self.pool)
        .await?;

        Ok(labels)
    }

    /// Set labels for a shared task (replaces existing)
    pub async fn set_task_labels(
        &self,
        shared_task_id: Uuid,
        label_ids: &[Uuid],
    ) -> Result<Vec<Label>, LabelError> {
        let mut tx = self.pool.begin().await?;

        // Delete existing task labels
        sqlx::query!(
            "DELETE FROM shared_task_labels WHERE shared_task_id = $1",
            shared_task_id
        )
        .execute(&mut *tx)
        .await?;

        // Insert new task labels
        for label_id in label_ids {
            sqlx::query!(
                "INSERT INTO shared_task_labels (shared_task_id, label_id) VALUES ($1, $2)",
                shared_task_id,
                label_id
            )
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;

        // Return the newly set labels
        self.find_by_task(shared_task_id).await
    }

    /// Add a label to a shared task
    pub async fn attach_to_task(
        &self,
        shared_task_id: Uuid,
        label_id: Uuid,
    ) -> Result<(), LabelError> {
        sqlx::query!(
            r#"
            INSERT INTO shared_task_labels (shared_task_id, label_id)
            VALUES ($1, $2)
            ON CONFLICT (shared_task_id, label_id) DO NOTHING
            "#,
            shared_task_id,
            label_id
        )
        .execute(self.pool)
        .await?;

        Ok(())
    }

    /// Remove a label from a shared task
    pub async fn detach_from_task(
        &self,
        shared_task_id: Uuid,
        label_id: Uuid,
    ) -> Result<u64, LabelError> {
        let result = sqlx::query!(
            "DELETE FROM shared_task_labels WHERE shared_task_id = $1 AND label_id = $2",
            shared_task_id,
            label_id
        )
        .execute(self.pool)
        .await?;

        Ok(result.rows_affected())
    }

    /// Find or create a label by name within a scope (used for conflict resolution)
    /// Returns the existing label if one with the same name exists, otherwise creates a new one.
    /// Uses INSERT ... ON CONFLICT to handle race conditions atomically.
    pub async fn find_or_create(&self, data: CreateLabelData) -> Result<(Label, bool), LabelError> {
        // Use INSERT ... ON CONFLICT DO NOTHING to atomically handle race conditions
        // If a conflict occurs (same org/project/name/deleted_at), we just fetch the existing one
        let result: Option<Uuid> = sqlx::query_scalar(
            r#"
            INSERT INTO labels (
                organization_id,
                project_id,
                origin_node_id,
                name,
                icon,
                color
            )
            VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (organization_id, project_id, name, deleted_at) DO NOTHING
            RETURNING id
            "#,
        )
        .bind(data.organization_id)
        .bind(data.project_id)
        .bind(data.origin_node_id)
        .bind(&data.name)
        .bind(&data.icon)
        .bind(&data.color)
        .fetch_optional(self.pool)
        .await?;

        if let Some(id) = result {
            // We successfully inserted, fetch the full label
            let label = self.find_by_id(id).await?.ok_or(LabelError::NotFound)?;
            return Ok((label, true));
        }

        // Conflict occurred, fetch the existing label
        let existing = sqlx::query_as::<_, Label>(
            r#"
            SELECT
                id,
                organization_id,
                project_id,
                origin_node_id,
                name,
                icon,
                color,
                version,
                deleted_at,
                created_at,
                updated_at
            FROM labels
            WHERE organization_id = $1
              AND ((project_id IS NULL AND $2::uuid IS NULL) OR project_id = $2)
              AND name = $3
              AND deleted_at IS NULL
            "#,
        )
        .bind(data.organization_id)
        .bind(data.project_id)
        .bind(&data.name)
        .fetch_one(self.pool)
        .await?;

        Ok((existing, false))
    }
}

/// Data for upserting a label from a node
#[derive(Debug, Clone)]
pub struct UpsertLabelFromNodeData {
    /// Shared label ID (if updating an existing label)
    pub shared_label_id: Option<Uuid>,
    pub organization_id: Uuid,
    pub project_id: Option<Uuid>,
    pub origin_node_id: Uuid,
    pub name: String,
    pub icon: String,
    pub color: String,
    pub version: i64,
}

impl LabelRepository<'_> {
    /// Get the organization_id for a label
    pub async fn organization_id(
        pool: &PgPool,
        label_id: Uuid,
    ) -> Result<Option<Uuid>, sqlx::Error> {
        sqlx::query_scalar!(
            r#"
            SELECT organization_id
            FROM labels
            WHERE id = $1
            "#,
            label_id
        )
        .fetch_optional(pool)
        .await
    }

    /// Upsert a label from a node with version-based conflict resolution.
    ///
    /// If the label is new (shared_label_id is None), creates a new label.
    /// If the label exists, updates it only if the incoming version is higher.
    /// Returns the label and whether it was created (true) or updated (false).
    pub async fn upsert_from_node(
        &self,
        data: UpsertLabelFromNodeData,
    ) -> Result<(Label, bool), LabelError> {
        // If shared_label_id is provided, try to update existing label
        if let Some(shared_label_id) = data.shared_label_id {
            // Update only if incoming version is higher or equal
            let updated = sqlx::query_as!(
                Label,
                r#"
                UPDATE labels AS l
                SET name           = $2,
                    icon           = $3,
                    color          = $4,
                    version        = $5,
                    origin_node_id = $6,
                    updated_at     = NOW()
                WHERE l.id = $1
                  AND l.version < $5
                  AND l.deleted_at IS NULL
                RETURNING
                    l.id              AS "id!",
                    l.organization_id AS "organization_id!",
                    l.project_id      AS "project_id?",
                    l.origin_node_id  AS "origin_node_id?",
                    l.name            AS "name!",
                    l.icon            AS "icon!",
                    l.color           AS "color!",
                    l.version         AS "version!",
                    l.deleted_at      AS "deleted_at?",
                    l.created_at      AS "created_at!",
                    l.updated_at      AS "updated_at!"
                "#,
                shared_label_id,
                data.name,
                data.icon,
                data.color,
                data.version,
                data.origin_node_id,
            )
            .fetch_optional(self.pool)
            .await?;

            if let Some(label) = updated {
                return Ok((label, false));
            }

            // If no update, label might have higher version already or doesn't exist
            // Try to fetch it
            if let Some(label) = self.find_by_id(shared_label_id).await? {
                // Label exists with same or higher version, no update needed
                return Ok((label, false));
            }

            // Label doesn't exist, fall through to create
        }

        // Create new label (or for new labels without shared_label_id)
        // Use find_or_create to handle race conditions where label already exists
        let (label, was_created) = self
            .find_or_create(CreateLabelData {
                organization_id: data.organization_id,
                project_id: data.project_id,
                origin_node_id: Some(data.origin_node_id),
                name: data.name.clone(),
                icon: data.icon.clone(),
                color: data.color.clone(),
            })
            .await?;

        // Update the version to match the node's version
        let label = sqlx::query_as!(
            Label,
            r#"
            UPDATE labels
            SET version = $2
            WHERE id = $1
            RETURNING
                id              AS "id!",
                organization_id AS "organization_id!",
                project_id      AS "project_id?",
                origin_node_id  AS "origin_node_id?",
                name            AS "name!",
                icon            AS "icon!",
                color           AS "color!",
                version         AS "version!",
                deleted_at      AS "deleted_at?",
                created_at      AS "created_at!",
                updated_at      AS "updated_at!"
            "#,
            label.id,
            data.version
        )
        .fetch_one(self.pool)
        .await?;

        Ok((label, was_created))
    }

    /// Merge two labels by moving all task associations from source to target.
    ///
    /// This operation:
    /// 1. Moves all task-label associations from source_id to target_id
    /// 2. Soft-deletes the source label
    ///
    /// Returns the target label and the number of migrated task associations.
    pub async fn merge_labels(
        &self,
        source_id: Uuid,
        target_id: Uuid,
    ) -> Result<(Label, u64), LabelError> {
        let mut tx = self.pool.begin().await?;

        // First, update task associations from source to target
        // Only move those where target doesn't already have the same task
        let result = sqlx::query(
            r#"
            WITH moved AS (
                UPDATE shared_task_labels
                SET label_id = $2
                WHERE label_id = $1
                  AND shared_task_id NOT IN (
                      SELECT shared_task_id FROM shared_task_labels WHERE label_id = $2
                  )
                RETURNING 1
            )
            SELECT COUNT(*)::bigint AS count FROM moved
            "#,
        )
        .bind(source_id)
        .bind(target_id)
        .fetch_one(&mut *tx)
        .await?;

        let migrated: i64 = result.get("count");

        // Delete remaining source associations (tasks that already had target label)
        sqlx::query("DELETE FROM shared_task_labels WHERE label_id = $1")
            .bind(source_id)
            .execute(&mut *tx)
            .await?;

        // Soft-delete the source label
        sqlx::query(
            r#"
            UPDATE labels
            SET deleted_at = NOW(),
                version = version + 1,
                updated_at = NOW()
            WHERE id = $1
              AND deleted_at IS NULL
            "#,
        )
        .bind(source_id)
        .execute(&mut *tx)
        .await?;

        // Fetch the target label
        let label = sqlx::query_as::<_, Label>(
            r#"
            SELECT
                id,
                organization_id,
                project_id,
                origin_node_id,
                name,
                icon,
                color,
                version,
                deleted_at,
                created_at,
                updated_at
            FROM labels
            WHERE id = $1
            "#,
        )
        .bind(target_id)
        .fetch_one(&mut *tx)
        .await?;

        tx.commit().await?;

        Ok((label, migrated as u64))
    }

    /// Promote a project-scoped label to a swarm label by setting project_id to NULL.
    ///
    /// This makes the label available across all projects in the organization.
    pub async fn promote_to_swarm(&self, label_id: Uuid) -> Result<Label, LabelError> {
        // Check if there's already a swarm label with the same name in the org
        let existing = self.find_by_id(label_id).await?;
        let label = existing.ok_or(LabelError::NotFound)?;

        if label.project_id.is_none() {
            // Already a swarm label
            return Ok(label);
        }

        // Check for name conflict with existing swarm labels
        let conflict = sqlx::query_scalar::<_, Uuid>(
            r#"
            SELECT id
            FROM labels
            WHERE organization_id = $1
              AND project_id IS NULL
              AND name = $2
              AND deleted_at IS NULL
              AND id != $3
            "#,
        )
        .bind(label.organization_id)
        .bind(&label.name)
        .bind(label_id)
        .fetch_optional(self.pool)
        .await?;

        if conflict.is_some() {
            return Err(LabelError::Conflict(format!(
                "A swarm label with name '{}' already exists",
                label.name
            )));
        }

        // Promote to swarm label
        let promoted = sqlx::query_as::<_, Label>(
            r#"
            UPDATE labels
            SET project_id = NULL,
                version = version + 1,
                updated_at = NOW()
            WHERE id = $1
              AND deleted_at IS NULL
            RETURNING
                id,
                organization_id,
                project_id,
                origin_node_id,
                name,
                icon,
                color,
                version,
                deleted_at,
                created_at,
                updated_at
            "#,
        )
        .bind(label_id)
        .fetch_optional(self.pool)
        .await?
        .ok_or(LabelError::NotFound)?;

        Ok(promoted)
    }
}
