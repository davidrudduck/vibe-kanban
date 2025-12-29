use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{Executor, FromRow, Sqlite, SqlitePool};
use ts_rs::TS;
use uuid::Uuid;

/// A label for visual task categorization
#[derive(Debug, Clone, FromRow, Serialize, Deserialize, TS)]
pub struct Label {
    pub id: Uuid,
    /// Project ID if project-specific, NULL if global
    pub project_id: Option<Uuid>,
    pub name: String,
    /// Lucide icon name (e.g., "tag", "bug", "code")
    pub icon: String,
    /// Hex color code (e.g., "#3b82f6")
    pub color: String,
    /// UUID of the label in the Hive (NULL if not yet synced)
    #[ts(optional)]
    pub shared_label_id: Option<Uuid>,
    /// Optimistic locking version for conflict resolution
    pub version: i64,
    /// Timestamp of last successful sync to Hive
    #[ts(optional)]
    pub synced_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Request to create a new label
#[derive(Debug, Deserialize, TS)]
pub struct CreateLabel {
    /// Project ID if project-specific, None for global label
    pub project_id: Option<Uuid>,
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

/// Request to update an existing label
#[derive(Debug, Deserialize, TS)]
pub struct UpdateLabel {
    pub name: Option<String>,
    pub icon: Option<String>,
    pub color: Option<String>,
}

/// Junction table entry for task-label relationships
#[derive(Debug, Clone, FromRow, Serialize, Deserialize, TS)]
pub struct TaskLabel {
    pub id: Uuid,
    pub task_id: Uuid,
    pub label_id: Uuid,
    pub created_at: DateTime<Utc>,
}

/// Request to set labels for a task (replaces existing)
#[derive(Debug, Deserialize, TS)]
pub struct SetTaskLabels {
    pub label_ids: Vec<Uuid>,
}

impl Label {
    /// Find all labels visible to a project (global + project-specific)
    pub async fn find_for_project(
        pool: &SqlitePool,
        project_id: Option<Uuid>,
    ) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as!(
            Label,
            r#"SELECT
                id as "id!: Uuid",
                project_id as "project_id: Uuid",
                name,
                icon,
                color,
                shared_label_id as "shared_label_id: Uuid",
                version as "version!: i64",
                synced_at as "synced_at: DateTime<Utc>",
                created_at as "created_at!: DateTime<Utc>",
                updated_at as "updated_at!: DateTime<Utc>"
            FROM labels
            WHERE project_id IS NULL OR project_id = $1
            ORDER BY name ASC"#,
            project_id
        )
        .fetch_all(pool)
        .await
    }

    /// Find all global labels
    pub async fn find_global(pool: &SqlitePool) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as!(
            Label,
            r#"SELECT
                id as "id!: Uuid",
                project_id as "project_id: Uuid",
                name,
                icon,
                color,
                shared_label_id as "shared_label_id: Uuid",
                version as "version!: i64",
                synced_at as "synced_at: DateTime<Utc>",
                created_at as "created_at!: DateTime<Utc>",
                updated_at as "updated_at!: DateTime<Utc>"
            FROM labels
            WHERE project_id IS NULL
            ORDER BY name ASC"#
        )
        .fetch_all(pool)
        .await
    }

    /// Find a label by ID
    pub async fn find_by_id(pool: &SqlitePool, id: Uuid) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as!(
            Label,
            r#"SELECT
                id as "id!: Uuid",
                project_id as "project_id: Uuid",
                name,
                icon,
                color,
                shared_label_id as "shared_label_id: Uuid",
                version as "version!: i64",
                synced_at as "synced_at: DateTime<Utc>",
                created_at as "created_at!: DateTime<Utc>",
                updated_at as "updated_at!: DateTime<Utc>"
            FROM labels
            WHERE id = $1"#,
            id
        )
        .fetch_optional(pool)
        .await
    }

    /// Create a new label
    pub async fn create(pool: &SqlitePool, data: &CreateLabel) -> Result<Self, sqlx::Error> {
        let id = Uuid::new_v4();
        sqlx::query_as!(
            Label,
            r#"INSERT INTO labels (id, project_id, name, icon, color)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING
                id as "id!: Uuid",
                project_id as "project_id: Uuid",
                name,
                icon,
                color,
                shared_label_id as "shared_label_id: Uuid",
                version as "version!: i64",
                synced_at as "synced_at: DateTime<Utc>",
                created_at as "created_at!: DateTime<Utc>",
                updated_at as "updated_at!: DateTime<Utc>""#,
            id,
            data.project_id,
            data.name,
            data.icon,
            data.color
        )
        .fetch_one(pool)
        .await
    }

    /// Update an existing label
    pub async fn update(
        pool: &SqlitePool,
        id: Uuid,
        data: &UpdateLabel,
    ) -> Result<Self, sqlx::Error> {
        let existing = Self::find_by_id(pool, id)
            .await?
            .ok_or(sqlx::Error::RowNotFound)?;

        let name = data.name.as_ref().unwrap_or(&existing.name);
        let icon = data.icon.as_ref().unwrap_or(&existing.icon);
        let color = data.color.as_ref().unwrap_or(&existing.color);

        sqlx::query_as!(
            Label,
            r#"UPDATE labels
            SET name = $2, icon = $3, color = $4, updated_at = datetime('now', 'subsec')
            WHERE id = $1
            RETURNING
                id as "id!: Uuid",
                project_id as "project_id: Uuid",
                name,
                icon,
                color,
                shared_label_id as "shared_label_id: Uuid",
                version as "version!: i64",
                synced_at as "synced_at: DateTime<Utc>",
                created_at as "created_at!: DateTime<Utc>",
                updated_at as "updated_at!: DateTime<Utc>""#,
            id,
            name,
            icon,
            color
        )
        .fetch_one(pool)
        .await
    }

    /// Delete a label
    pub async fn delete(pool: &SqlitePool, id: Uuid) -> Result<u64, sqlx::Error> {
        let result = sqlx::query!("DELETE FROM labels WHERE id = $1", id)
            .execute(pool)
            .await?;
        Ok(result.rows_affected())
    }

    /// Get all labels for a specific task
    pub async fn find_by_task_id(
        pool: &SqlitePool,
        task_id: Uuid,
    ) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as!(
            Label,
            r#"SELECT
                l.id as "id!: Uuid",
                l.project_id as "project_id: Uuid",
                l.name,
                l.icon,
                l.color,
                l.shared_label_id as "shared_label_id: Uuid",
                l.version as "version!: i64",
                l.synced_at as "synced_at: DateTime<Utc>",
                l.created_at as "created_at!: DateTime<Utc>",
                l.updated_at as "updated_at!: DateTime<Utc>"
            FROM labels l
            INNER JOIN task_labels tl ON l.id = tl.label_id
            WHERE tl.task_id = $1
            ORDER BY l.name ASC"#,
            task_id
        )
        .fetch_all(pool)
        .await
    }

    /// Set labels for a task (replaces existing labels)
    pub async fn set_task_labels(
        pool: &SqlitePool,
        task_id: Uuid,
        label_ids: &[Uuid],
    ) -> Result<Vec<Self>, sqlx::Error> {
        // Delete existing task labels
        sqlx::query!("DELETE FROM task_labels WHERE task_id = $1", task_id)
            .execute(pool)
            .await?;

        // Insert new task labels
        for label_id in label_ids {
            let id = Uuid::new_v4();
            sqlx::query!(
                "INSERT INTO task_labels (id, task_id, label_id) VALUES ($1, $2, $3)",
                id,
                task_id,
                label_id
            )
            .execute(pool)
            .await?;
        }

        // Return the newly set labels
        Self::find_by_task_id(pool, task_id).await
    }

    /// Add a label to a task
    pub async fn attach_to_task(
        pool: &SqlitePool,
        task_id: Uuid,
        label_id: Uuid,
    ) -> Result<(), sqlx::Error> {
        let id = Uuid::new_v4();
        sqlx::query!(
            "INSERT OR IGNORE INTO task_labels (id, task_id, label_id) VALUES ($1, $2, $3)",
            id,
            task_id,
            label_id
        )
        .execute(pool)
        .await?;
        Ok(())
    }

    /// Remove a label from a task
    pub async fn detach_from_task(
        pool: &SqlitePool,
        task_id: Uuid,
        label_id: Uuid,
    ) -> Result<u64, sqlx::Error> {
        let result = sqlx::query!(
            "DELETE FROM task_labels WHERE task_id = $1 AND label_id = $2",
            task_id,
            label_id
        )
        .execute(pool)
        .await?;
        Ok(result.rows_affected())
    }

    // ============================================================
    // Sync-related methods for Hive integration
    // ============================================================

    /// Find all labels that haven't been synced to the Hive yet
    pub async fn find_unsynced(pool: &SqlitePool) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as!(
            Label,
            r#"SELECT
                id as "id!: Uuid",
                project_id as "project_id: Uuid",
                name,
                icon,
                color,
                shared_label_id as "shared_label_id: Uuid",
                version as "version!: i64",
                synced_at as "synced_at: DateTime<Utc>",
                created_at as "created_at!: DateTime<Utc>",
                updated_at as "updated_at!: DateTime<Utc>"
            FROM labels
            WHERE shared_label_id IS NULL
            ORDER BY created_at ASC"#
        )
        .fetch_all(pool)
        .await
    }

    /// Find a label by its Hive shared_label_id
    pub async fn find_by_shared_label_id<'e, E>(
        executor: E,
        shared_label_id: Uuid,
    ) -> Result<Option<Self>, sqlx::Error>
    where
        E: Executor<'e, Database = Sqlite>,
    {
        sqlx::query_as!(
            Label,
            r#"SELECT
                id as "id!: Uuid",
                project_id as "project_id: Uuid",
                name,
                icon,
                color,
                shared_label_id as "shared_label_id: Uuid",
                version as "version!: i64",
                synced_at as "synced_at: DateTime<Utc>",
                created_at as "created_at!: DateTime<Utc>",
                updated_at as "updated_at!: DateTime<Utc>"
            FROM labels
            WHERE shared_label_id = $1"#,
            shared_label_id
        )
        .fetch_optional(executor)
        .await
    }

    /// Set the shared_label_id after syncing to Hive
    pub async fn set_shared_label_id(
        pool: &SqlitePool,
        id: Uuid,
        shared_label_id: Uuid,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"UPDATE labels
            SET shared_label_id = $2, synced_at = datetime('now', 'subsec'), updated_at = datetime('now', 'subsec')
            WHERE id = $1"#,
            id,
            shared_label_id
        )
        .execute(pool)
        .await?;
        Ok(())
    }

    /// Mark a label as synced (update synced_at timestamp)
    pub async fn mark_synced(pool: &SqlitePool, id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"UPDATE labels
            SET synced_at = datetime('now', 'subsec'), updated_at = datetime('now', 'subsec')
            WHERE id = $1"#,
            id
        )
        .execute(pool)
        .await?;
        Ok(())
    }

    /// Clear the shared_label_id (unlink from Hive)
    pub async fn clear_shared_label_id<'e, E>(executor: E, id: Uuid) -> Result<(), sqlx::Error>
    where
        E: Executor<'e, Database = Sqlite>,
    {
        sqlx::query!(
            r#"UPDATE labels
            SET shared_label_id = NULL, synced_at = NULL, updated_at = datetime('now', 'subsec')
            WHERE id = $1"#,
            id
        )
        .execute(executor)
        .await?;
        Ok(())
    }

    /// Find labels that have been modified since last sync
    pub async fn find_modified_since_sync(pool: &SqlitePool) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as!(
            Label,
            r#"SELECT
                id as "id!: Uuid",
                project_id as "project_id: Uuid",
                name,
                icon,
                color,
                shared_label_id as "shared_label_id: Uuid",
                version as "version!: i64",
                synced_at as "synced_at: DateTime<Utc>",
                created_at as "created_at!: DateTime<Utc>",
                updated_at as "updated_at!: DateTime<Utc>"
            FROM labels
            WHERE shared_label_id IS NOT NULL
              AND updated_at > synced_at
            ORDER BY updated_at ASC"#
        )
        .fetch_all(pool)
        .await
    }

    /// Increment the version for optimistic locking
    pub async fn increment_version(pool: &SqlitePool, id: Uuid) -> Result<i64, sqlx::Error> {
        let result = sqlx::query!(
            r#"UPDATE labels
            SET version = version + 1, updated_at = datetime('now', 'subsec')
            WHERE id = $1
            RETURNING version as "version!: i64""#,
            id
        )
        .fetch_one(pool)
        .await?;
        Ok(result.version)
    }

    /// Create a label from a Hive sync (with shared_label_id already set)
    pub async fn create_from_hive<'e, E>(
        executor: E,
        shared_label_id: Uuid,
        project_id: Option<Uuid>,
        name: &str,
        icon: &str,
        color: &str,
        version: i64,
    ) -> Result<Self, sqlx::Error>
    where
        E: Executor<'e, Database = Sqlite>,
    {
        let id = Uuid::new_v4();
        sqlx::query_as!(
            Label,
            r#"INSERT INTO labels (id, project_id, name, icon, color, shared_label_id, version, synced_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, datetime('now', 'subsec'))
            RETURNING
                id as "id!: Uuid",
                project_id as "project_id: Uuid",
                name,
                icon,
                color,
                shared_label_id as "shared_label_id: Uuid",
                version as "version!: i64",
                synced_at as "synced_at: DateTime<Utc>",
                created_at as "created_at!: DateTime<Utc>",
                updated_at as "updated_at!: DateTime<Utc>""#,
            id,
            project_id,
            name,
            icon,
            color,
            shared_label_id,
            version
        )
        .fetch_one(executor)
        .await
    }

    /// Update a label from Hive sync data
    pub async fn update_from_hive<'e, E>(
        executor: E,
        id: Uuid,
        name: &str,
        icon: &str,
        color: &str,
        version: i64,
    ) -> Result<Self, sqlx::Error>
    where
        E: Executor<'e, Database = Sqlite>,
    {
        sqlx::query_as!(
            Label,
            r#"UPDATE labels
            SET name = $2, icon = $3, color = $4, version = $5, synced_at = datetime('now', 'subsec'), updated_at = datetime('now', 'subsec')
            WHERE id = $1
            RETURNING
                id as "id!: Uuid",
                project_id as "project_id: Uuid",
                name,
                icon,
                color,
                shared_label_id as "shared_label_id: Uuid",
                version as "version!: i64",
                synced_at as "synced_at: DateTime<Utc>",
                created_at as "created_at!: DateTime<Utc>",
                updated_at as "updated_at!: DateTime<Utc>""#,
            id,
            name,
            icon,
            color,
            version
        )
        .fetch_one(executor)
        .await
    }
}
