use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use thiserror::Error;
use uuid::Uuid;

use super::{
    Tx,
    identity_errors::IdentityError,
    users::{UserData, fetch_user},
};
use crate::db::maintenance;

pub struct BulkFetchResult {
    pub tasks: Vec<SharedTaskActivityPayload>,
    pub deleted_task_ids: Vec<Uuid>,
    pub latest_seq: Option<i64>,
}

pub const MAX_SHARED_TASK_TEXT_BYTES: usize = 50 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "kebab-case")]
#[sqlx(type_name = "task_status", rename_all = "kebab-case")]
pub enum TaskStatus {
    Todo,
    InProgress,
    InReview,
    Done,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SharedTaskWithUser {
    pub task: SharedTask,
    pub user: Option<UserData>,
}

impl SharedTaskWithUser {
    pub fn new(task: SharedTask, user: Option<UserData>) -> Self {
        Self { task, user }
    }
}

/// Shared task struct with optional denormalized metadata for API responses.
///
/// This struct serves dual purposes:
/// 1. Database queries (core fields come from shared_tasks table)
/// 2. API responses (includes denormalized assignee metadata)
///
/// The assignee metadata fields (`assignee_name`, `assignee_username`, `activity_at`)
/// are populated separately after fetching from the database, typically by joining
/// with the users table or using cached user data.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct SharedTask {
    pub id: Uuid,
    pub organization_id: Uuid,
    /// Legacy project ID (nullable after migration, swarm_project_id is the source of truth)
    pub project_id: Option<Uuid>,
    /// Swarm project ID for tasks synced via swarm (the source of truth)
    pub swarm_project_id: Option<Uuid>,
    pub creator_user_id: Option<Uuid>,
    pub assignee_user_id: Option<Uuid>,
    pub deleted_by_user_id: Option<Uuid>,
    pub executing_node_id: Option<Uuid>,
    /// Node currently owning/working on this task
    pub owner_node_id: Option<Uuid>,
    /// Name of the owner node (denormalized for display)
    pub owner_name: Option<String>,
    /// Original local task ID from source node, used for re-sync duplicate detection
    pub source_task_id: Option<Uuid>,
    /// Node that originally created this task, used with source_task_id for uniqueness
    pub source_node_id: Option<Uuid>,
    pub title: String,
    pub description: Option<String>,
    pub status: TaskStatus,
    pub version: i64,
    pub deleted_at: Option<DateTime<Utc>>,
    pub shared_at: Option<DateTime<Utc>>,
    pub archived_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,

    // Denormalized assignee metadata for cross-node display
    // These fields are NOT stored in the database - they are populated separately
    // after fetching tasks, using #[sqlx(default)] to allow FromRow derivation.
    /// Display name of the assignee (first + last name)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[sqlx(default)]
    pub assignee_name: Option<String>,
    /// Username of the assignee (for badge display)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[sqlx(default)]
    pub assignee_username: Option<String>,
    /// Timestamp of last status change (for time-in-column tracking)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[sqlx(default)]
    pub activity_at: Option<DateTime<Utc>>,
}

impl SharedTask {
    /// Populate assignee metadata from user data.
    pub fn with_assignee_info(mut self, user: Option<&UserData>) -> Self {
        if let Some(u) = user {
            // Build display name from first + last name
            self.assignee_name = match (&u.first_name, &u.last_name) {
                (Some(first), Some(last)) => Some(format!("{} {}", first, last)),
                (Some(first), None) => Some(first.clone()),
                (None, Some(last)) => Some(last.clone()),
                (None, None) => None,
            };
            self.assignee_username = u.username.clone();
        }
        self
    }

    /// Set activity_at to track when status last changed.
    /// For tasks fetched from the Hive, we use updated_at as a proxy for activity.
    pub fn with_activity_at(mut self, activity_at: Option<DateTime<Utc>>) -> Self {
        self.activity_at = activity_at;
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SharedTaskActivityPayload {
    pub task: SharedTask,
    pub user: Option<UserData>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateSharedTaskData {
    pub organization_id: Uuid,
    pub project_id: Option<Uuid>,
    pub title: String,
    pub description: Option<String>,
    pub status: Option<TaskStatus>,
    pub creator_user_id: Uuid,
    pub assignee_user_id: Option<Uuid>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateSharedTaskData {
    pub title: Option<String>,
    pub description: Option<String>,
    pub status: Option<TaskStatus>,
    pub archived_at: Option<Option<DateTime<Utc>>>,
    pub version: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AssignTaskData {
    pub new_assignee_user_id: Option<Uuid>,
    pub previous_assignee_user_id: Option<Uuid>,
    pub version: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DeleteTaskData {
    pub acting_user_id: Uuid,
    pub version: Option<i64>,
}

/// Data for creating or updating a shared task from a node.
///
/// This is used when nodes sync locally-created tasks to the hive.
/// Unlike `CreateSharedTaskData`, this doesn't require a user ID.
#[derive(Debug, Clone)]
pub struct UpsertTaskFromNodeData {
    /// Swarm project ID (the new single source of truth)
    pub swarm_project_id: Uuid,
    /// Legacy project ID (for backwards compatibility, same as swarm_project_id for new tasks)
    pub project_id: Uuid,
    /// Organization ID (from the project)
    pub organization_id: Uuid,
    /// Node that owns this task
    pub origin_node_id: Uuid,
    /// Local task ID on the source node
    pub local_task_id: Uuid,
    /// Title of the task
    pub title: String,
    /// Description of the task
    pub description: Option<String>,
    /// Task status
    pub status: TaskStatus,
    /// Version for conflict resolution
    pub version: i64,
    /// Node currently owning/working on this task
    pub owner_node_id: Option<Uuid>,
    /// Name of the owner node
    pub owner_name: Option<String>,
}

#[derive(Debug, Error)]
pub enum SharedTaskError {
    #[error("shared task not found")]
    NotFound,
    #[error("operation forbidden")]
    Forbidden,
    #[error("shared task conflict: {0}")]
    Conflict(String),
    #[error("shared task title and description are too large")]
    PayloadTooLarge,
    #[error(transparent)]
    Identity(#[from] IdentityError),
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error(transparent)]
    Serialization(#[from] serde_json::Error),
}

pub struct SharedTaskRepository<'a> {
    pool: &'a PgPool,
}

impl<'a> SharedTaskRepository<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    /// Fetches a shared task by its ID, excluding tasks that have been marked deleted.
    ///
    /// Returns `Some(SharedTask)` if a non-deleted task with the given `task_id` exists, `None` otherwise.
    ///
    /// # Examples
    ///
    /// ```
    /// # use uuid::Uuid;
    /// # async fn example(repo: &crate::SharedTaskRepository<'_>, id: Uuid) {
    /// let result = repo.find_by_id(id).await.unwrap();
    /// if let Some(task) = result {
    ///     println!("Found task {}", task.id);
    /// } else {
    ///     println!("Task not found or deleted");
    /// }
    /// # }
    /// ```
    pub async fn find_by_id(&self, task_id: Uuid) -> Result<Option<SharedTask>, SharedTaskError> {
        let task = sqlx::query_as!(
            SharedTask,
            r#"
            SELECT
                id                  AS "id!",
                organization_id     AS "organization_id!: Uuid",
                project_id          AS "project_id?: Uuid",
                swarm_project_id    AS "swarm_project_id?: Uuid",
                creator_user_id     AS "creator_user_id?: Uuid",
                assignee_user_id    AS "assignee_user_id?: Uuid",
                deleted_by_user_id  AS "deleted_by_user_id?: Uuid",
                executing_node_id   AS "executing_node_id?: Uuid",
                owner_node_id       AS "owner_node_id?: Uuid",
                owner_name          AS "owner_name?",
                source_task_id      AS "source_task_id?: Uuid",
                source_node_id      AS "source_node_id?: Uuid",
                title               AS "title!",
                description         AS "description?",
                status              AS "status!: TaskStatus",
                version             AS "version!",
                deleted_at          AS "deleted_at?",
                shared_at           AS "shared_at?",
                archived_at         AS "archived_at?",
                created_at          AS "created_at!",
                updated_at          AS "updated_at!",
                NULL::text          AS "assignee_name?",
                NULL::text          AS "assignee_username?",
                NULL::timestamptz   AS "activity_at?"
            FROM shared_tasks
            WHERE id = $1
              AND deleted_at IS NULL
            "#,
            task_id
        )
        .fetch_optional(self.pool)
        .await?;

        Ok(task)
    }

    /// Fetches all shared tasks for a swarm project, excluding deleted tasks.
    ///
    /// Returns tasks ordered by updated_at descending (most recently updated first).
    pub async fn find_by_swarm_project_id(
        &self,
        swarm_project_id: Uuid,
    ) -> Result<Vec<SharedTask>, SharedTaskError> {
        let tasks = sqlx::query_as::<_, SharedTask>(
            r#"
            SELECT
                id,
                organization_id,
                project_id,
                swarm_project_id,
                creator_user_id,
                assignee_user_id,
                deleted_by_user_id,
                executing_node_id,
                owner_node_id,
                owner_name,
                source_task_id,
                source_node_id,
                title,
                description,
                status,
                version,
                deleted_at,
                shared_at,
                archived_at,
                created_at,
                updated_at
            FROM shared_tasks
            WHERE swarm_project_id = $1
              AND deleted_at IS NULL
            ORDER BY updated_at DESC
            "#,
        )
        .bind(swarm_project_id)
        .fetch_all(self.pool)
        .await?;

        Ok(tasks)
    }

    /// Find a shared task by its source task ID and source node ID.
    ///
    /// Returns `Some(SharedTask)` if a non-deleted task exists that was created from the same source, `None` otherwise.
    ///
    /// This relies on the unique constraint on `(source_node_id, source_task_id)` to detect duplicates.
    ///
    /// # Examples
    ///
    /// ```
    /// # async fn example(repo: &SharedTaskRepository<'_>, node_id: uuid::Uuid, task_id: uuid::Uuid) -> Result<(), SharedTaskError> {
    /// let found = repo.find_by_source_task_id(node_id, task_id).await?;
    /// if let Some(task) = found {
    ///     println!("Found task: {}", task.id);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn find_by_source_task_id(
        &self,
        source_node_id: Uuid,
        source_task_id: Uuid,
    ) -> Result<Option<SharedTask>, SharedTaskError> {
        let task = sqlx::query_as!(
            SharedTask,
            r#"
            SELECT
                id                  AS "id!",
                organization_id     AS "organization_id!: Uuid",
                project_id          AS "project_id?: Uuid",
                swarm_project_id    AS "swarm_project_id?: Uuid",
                creator_user_id     AS "creator_user_id?: Uuid",
                assignee_user_id    AS "assignee_user_id?: Uuid",
                deleted_by_user_id  AS "deleted_by_user_id?: Uuid",
                executing_node_id   AS "executing_node_id?: Uuid",
                owner_node_id       AS "owner_node_id?: Uuid",
                owner_name          AS "owner_name?",
                source_task_id      AS "source_task_id?: Uuid",
                source_node_id      AS "source_node_id?: Uuid",
                title               AS "title!",
                description         AS "description?",
                status              AS "status!: TaskStatus",
                version             AS "version!",
                deleted_at          AS "deleted_at?",
                shared_at           AS "shared_at?",
                archived_at         AS "archived_at?",
                created_at          AS "created_at!",
                updated_at          AS "updated_at!",
                NULL::text          AS "assignee_name?",
                NULL::text          AS "assignee_username?",
                NULL::timestamptz   AS "activity_at?"
            FROM shared_tasks
            WHERE source_node_id = $1
              AND source_task_id = $2
              AND deleted_at IS NULL
            "#,
            source_node_id,
            source_task_id
        )
        .fetch_optional(self.pool)
        .await?;

        Ok(task)
    }

    /// Update the source tracking fields on a task.
    ///
    /// This is used to associate a task with its source node and task ID
    /// for duplicate detection during re-sync.
    pub async fn set_source_task_id(
        &self,
        task_id: Uuid,
        source_node_id: Uuid,
        source_task_id: Uuid,
    ) -> Result<(), SharedTaskError> {
        sqlx::query!(
            r#"
            UPDATE shared_tasks
            SET source_node_id = $2, source_task_id = $3
            WHERE id = $1
            "#,
            task_id,
            source_node_id,
            source_task_id
        )
        .execute(self.pool)
        .await?;
        Ok(())
    }

    /// Creates a new shared task, validates text size, persists it, records a `task.created` activity, and returns the created task with its optional assignee user.
    ///
    /// On success the returned value contains the newly inserted `SharedTask` and the assignee's `UserData` when an assignee was provided.
    ///
    /// # Examples
    ///
    /// ```
    /// # use uuid::Uuid;
    /// # use chrono::Utc;
    /// # async fn example(repo: &crate::SharedTaskRepository<'_>) -> Result<(), crate::SharedTaskError> {
    /// let data = crate::CreateSharedTaskData {
    ///     organization_id: Uuid::new_v4(),
    ///     project_id: None,
    ///     title: "Write docs".into(),
    ///     description: Some("Draft API docs for tasks".into()),
    ///     status: crate::TaskStatus::Todo,
    ///     creator_user_id: Some(Uuid::new_v4()),
    ///     assignee_user_id: None,
    /// };
    /// let created = repo.create(data).await?;
    /// assert_eq!(created.task.title, "Write docs");
    /// # Ok(()) }
    /// ```
    pub async fn create(
        &self,
        data: CreateSharedTaskData,
    ) -> Result<SharedTaskWithUser, SharedTaskError> {
        let mut tx = self.pool.begin().await.map_err(SharedTaskError::from)?;

        let CreateSharedTaskData {
            organization_id,
            project_id,
            title,
            description,
            status,
            creator_user_id,
            assignee_user_id,
        } = data;

        ensure_text_size(&title, description.as_deref())?;

        let task = sqlx::query_as!(
            SharedTask,
            r#"
            INSERT INTO shared_tasks (
                organization_id,
                project_id,
                swarm_project_id,
                creator_user_id,
                assignee_user_id,
                title,
                description,
                status,
                shared_at
            )
            VALUES ($1, $2, $2, $3, $4, $5, $6, COALESCE($7, 'todo'::task_status), NOW())
            RETURNING id                 AS "id!",
                      organization_id    AS "organization_id!: Uuid",
                      project_id         AS "project_id?: Uuid",
                      swarm_project_id   AS "swarm_project_id?: Uuid",
                      creator_user_id    AS "creator_user_id?: Uuid",
                      assignee_user_id   AS "assignee_user_id?: Uuid",
                      deleted_by_user_id AS "deleted_by_user_id?: Uuid",
                      executing_node_id  AS "executing_node_id?: Uuid",
                      owner_node_id      AS "owner_node_id?: Uuid",
                      owner_name         AS "owner_name?",
                      source_task_id     AS "source_task_id?: Uuid",
                      source_node_id     AS "source_node_id?: Uuid",
                      title              AS "title!",
                      description        AS "description?",
                      status             AS "status!: TaskStatus",
                      version            AS "version!",
                      deleted_at         AS "deleted_at?",
                      shared_at          AS "shared_at?",
                      archived_at        AS "archived_at?",
                      created_at         AS "created_at!",
                      updated_at         AS "updated_at!",
                      NULL::text         AS "assignee_name?",
                      NULL::text         AS "assignee_username?",
                      NULL::timestamptz  AS "activity_at?"
            "#,
            organization_id,
            project_id,
            creator_user_id,
            assignee_user_id,
            title,
            description,
            status as Option<TaskStatus>
        )
        .fetch_one(&mut *tx)
        .await?;

        let user = match assignee_user_id {
            Some(user_id) => fetch_user(&mut tx, user_id).await?,
            None => None,
        };

        insert_activity(&mut tx, &task, user.as_ref(), "task.created").await?;
        tx.commit().await.map_err(SharedTaskError::from)?;
        Ok(SharedTaskWithUser::new(task, user))
    }

    /// Upserts a shared task originating from a remote node, creating it if missing or updating the existing row when the same source identifiers are present.
    ///
    /// The operation is idempotent for the same (source_node_id, source_task_id) pair and will increment the stored version on update. The returned boolean is `true` when the call resulted in a new insert and `false` when an existing task was updated.
    ///
    /// # Examples
    ///
    /// ```
    /// # async fn example(repo: &crate::SharedTaskRepository<'_>) -> Result<(), crate::SharedTaskError> {
    /// use uuid::Uuid;
    /// use crate::{UpsertTaskFromNodeData, TaskStatus};
    ///
    /// let data = UpsertTaskFromNodeData {
    ///     swarm_project_id: Uuid::new_v4(),
    ///     project_id: None,
    ///     organization_id: Uuid::new_v4(),
    ///     origin_node_id: Some(Uuid::new_v4()),
    ///     local_task_id: Some("remote-1".to_string()),
    ///     title: "Sync task".into(),
    ///     description: Some("Created on remote node".into()),
    ///     status: TaskStatus::Todo,
    ///     version: 1,
    ///     owner_node_id: None,
    ///     owner_name: None,
    /// };
    ///
    /// let (task, was_created) = repo.upsert_from_node(data).await?;
    /// // `was_created` is true for a new insert, false for an update
    /// assert_eq!(was_created, task.version == 1);
    /// # Ok(()) }
    /// ```
    pub async fn upsert_from_node(
        &self,
        data: UpsertTaskFromNodeData,
    ) -> Result<(SharedTask, bool), SharedTaskError> {
        ensure_text_size(&data.title, data.description.as_deref())?;

        // Use ON CONFLICT to handle duplicate syncs (same source_node_id + source_task_id)
        // The unique constraint idx_shared_tasks_source_unique ensures idempotency
        let task: SharedTask = sqlx::query_as(
            r#"
            INSERT INTO shared_tasks (
                organization_id,
                project_id,
                swarm_project_id,
                executing_node_id,
                owner_node_id,
                owner_name,
                source_task_id,
                source_node_id,
                title,
                description,
                status,
                version,
                shared_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11::task_status, $12, NOW())
            ON CONFLICT (source_node_id, source_task_id)
                WHERE source_node_id IS NOT NULL AND source_task_id IS NOT NULL AND deleted_at IS NULL
            DO UPDATE SET
                title = EXCLUDED.title,
                description = EXCLUDED.description,
                status = EXCLUDED.status,
                owner_node_id = EXCLUDED.owner_node_id,
                owner_name = EXCLUDED.owner_name,
                version = shared_tasks.version + 1,
                updated_at = NOW()
            RETURNING id,
                      organization_id,
                      project_id,
                      swarm_project_id,
                      creator_user_id,
                      assignee_user_id,
                      deleted_by_user_id,
                      executing_node_id,
                      owner_node_id,
                      owner_name,
                      source_task_id,
                      source_node_id,
                      title,
                      description,
                      status,
                      version,
                      deleted_at,
                      shared_at,
                      archived_at,
                      created_at,
                      updated_at
            "#,
        )
        .bind(data.organization_id)
        .bind(data.project_id)
        .bind(data.swarm_project_id)
        .bind(data.origin_node_id) // executing_node_id
        .bind(data.owner_node_id)
        .bind(&data.owner_name)
        .bind(data.local_task_id) // source_task_id = local_task_id
        .bind(data.origin_node_id) // source_node_id = origin_node_id
        .bind(&data.title)
        .bind(&data.description)
        .bind(data.status)
        .bind(data.version)
        .fetch_one(self.pool)
        .await?;

        // Determine if this was a new insert or an update
        // A new insert will have version == data.version, an update will have version > data.version
        let was_created = task.version == data.version;

        tracing::debug!(
            task_id = %task.id,
            swarm_project_id = ?task.swarm_project_id,
            project_id = ?task.project_id,
            version = %task.version,
            source_task_id = ?task.source_task_id,
            was_created = was_created,
            "upserted shared task from node"
        );

        Ok((task, was_created))
    }

    /// Fetches all shared tasks for a project, the IDs of tasks that were deleted, and the latest activity sequence for that project.
    ///
    /// The returned BulkFetchResult contains:
    /// - `tasks`: a list of SharedTaskActivityPayload for each non-deleted task (task + optional assignee user),
    /// - `deleted_task_ids`: IDs of tasks that have been deleted for the project,
    /// - `latest_seq`: the highest activity sequence number for the project (or `None` if there are no activities).
    ///
    /// # Examples
    ///
    /// ```
    /// # use uuid::Uuid;
    /// # async fn example(repo: &crate::SharedTaskRepository<'_>, project_id: Uuid) {
    /// let result = repo.bulk_fetch(project_id).await.unwrap();
    /// // inspect results
    /// let _tasks = result.tasks;
    /// let _deleted = result.deleted_task_ids;
    /// let _latest_seq = result.latest_seq;
    /// # }
    /// ```
    pub async fn bulk_fetch(&self, project_id: Uuid) -> Result<BulkFetchResult, SharedTaskError> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ")
            .execute(&mut *tx)
            .await?;

        let rows = sqlx::query!(
            r#"
            SELECT
                st.id                     AS "id!: Uuid",
                st.organization_id        AS "organization_id!: Uuid",
                st.project_id             AS "project_id?: Uuid",
                st.swarm_project_id       AS "swarm_project_id?: Uuid",
                st.creator_user_id        AS "creator_user_id?: Uuid",
                st.assignee_user_id       AS "assignee_user_id?: Uuid",
                st.deleted_by_user_id     AS "deleted_by_user_id?: Uuid",
                st.executing_node_id      AS "executing_node_id?: Uuid",
                st.owner_node_id          AS "owner_node_id?: Uuid",
                st.owner_name             AS "owner_name?",
                st.source_task_id         AS "source_task_id?: Uuid",
                st.source_node_id         AS "source_node_id?: Uuid",
                st.title                  AS "title!",
                st.description            AS "description?",
                st.status                 AS "status!: TaskStatus",
                st.version                AS "version!",
                st.deleted_at             AS "deleted_at?",
                st.shared_at              AS "shared_at?",
                st.archived_at            AS "archived_at?",
                st.created_at             AS "created_at!",
                st.updated_at             AS "updated_at!",
                u.id                      AS "user_id?: Uuid",
                u.first_name              AS "user_first_name?",
                u.last_name               AS "user_last_name?",
                u.username                AS "user_username?"
            FROM shared_tasks st
            LEFT JOIN users u ON st.assignee_user_id = u.id
            WHERE st.project_id = $1
              AND st.deleted_at IS NULL
            ORDER BY st.updated_at DESC
            "#,
            project_id
        )
        .fetch_all(&mut *tx)
        .await?;

        let tasks = rows
            .into_iter()
            .map(|row| {
                let user = row.user_id.map(|id| UserData {
                    id,
                    first_name: row.user_first_name.clone(),
                    last_name: row.user_last_name.clone(),
                    username: row.user_username.clone(),
                });

                let task = SharedTask {
                    id: row.id,
                    organization_id: row.organization_id,
                    project_id: row.project_id,
                    swarm_project_id: row.swarm_project_id,
                    creator_user_id: row.creator_user_id,
                    assignee_user_id: row.assignee_user_id,
                    deleted_by_user_id: row.deleted_by_user_id,
                    executing_node_id: row.executing_node_id,
                    owner_node_id: row.owner_node_id,
                    owner_name: row.owner_name,
                    source_task_id: row.source_task_id,
                    source_node_id: row.source_node_id,
                    title: row.title,
                    description: row.description,
                    status: row.status,
                    version: row.version,
                    deleted_at: row.deleted_at,
                    shared_at: row.shared_at,
                    archived_at: row.archived_at,
                    created_at: row.created_at,
                    updated_at: row.updated_at,
                    // Initialize metadata fields with defaults, then populate via helpers
                    assignee_name: None,
                    assignee_username: None,
                    activity_at: None,
                }
                .with_assignee_info(user.as_ref())
                .with_activity_at(Some(row.updated_at));

                SharedTaskActivityPayload { task, user }
            })
            .collect();

        let deleted_rows = sqlx::query!(
            r#"
            SELECT st.id AS "id!: Uuid"
            FROM shared_tasks st
            WHERE st.project_id = $1
              AND st.deleted_at IS NOT NULL
            "#,
            project_id
        )
        .fetch_all(&mut *tx)
        .await?;

        let deleted_task_ids = deleted_rows.into_iter().map(|row| row.id).collect();

        let latest_seq = sqlx::query_scalar!(
            r#"
            SELECT MAX(seq)
            FROM activity
            WHERE project_id = $1
            "#,
            project_id
        )
        .fetch_one(&mut *tx)
        .await?;

        tx.commit().await?;

        Ok(BulkFetchResult {
            tasks,
            deleted_task_ids,
            latest_seq,
        })
    }

    /// Updates an existing shared task's mutable fields, records a "task.updated" activity, and returns the updated task with its assignee if any.
    ///
    /// The `archived_at` field in `UpdateSharedTaskData` is interpreted as: outer `None` = do not change, `Some(None)` = clear `archived_at`, `Some(Some(ts))` = set `archived_at` to `ts`.
    ///
    /// # Examples
    ///
    /// ```
    /// // Example usage in an async context:
    /// # async fn _example(repo: &crate::SharedTaskRepository<'_>, task_id: uuid::Uuid, data: crate::UpdateSharedTaskData) -> Result<(), crate::SharedTaskError> {
    /// let result = repo.update(task_id, data).await?;
    /// println!("updated task id = {}", result.task.id);
    /// # Ok(()) }
    /// ```
    ///
    /// # Returns
    ///
    /// `SharedTaskWithUser` containing the updated `SharedTask` and `Some(UserData)` when an assignee exists, or `None` for the assignee.
    pub async fn update(
        &self,
        task_id: Uuid,
        data: UpdateSharedTaskData,
    ) -> Result<SharedTaskWithUser, SharedTaskError> {
        let mut tx = self.pool.begin().await.map_err(SharedTaskError::from)?;

        // Flatten Option<Option<DateTime<Utc>>> for archived_at:
        // - None (outer): don't update archived_at
        // - Some(None): set archived_at to NULL (unarchive)
        // - Some(Some(ts)): set archived_at to timestamp (archive)
        let (should_update_archived, archived_at_value) = match &data.archived_at {
            None => (false, None),
            Some(inner) => (true, *inner),
        };

        let task = sqlx::query_as!(
            SharedTask,
            r#"
        UPDATE shared_tasks AS t
        SET title       = COALESCE($2, t.title),
            description = COALESCE($3, t.description),
            status      = COALESCE($4, t.status),
            archived_at = CASE WHEN $6 THEN $7 ELSE t.archived_at END,
            version     = t.version + 1,
            updated_at  = NOW()
        WHERE t.id = $1
          AND t.version = COALESCE($5, t.version)
          AND t.deleted_at IS NULL
        RETURNING
            t.id                AS "id!",
            t.organization_id   AS "organization_id!: Uuid",
            t.project_id        AS "project_id?: Uuid",
            t.swarm_project_id  AS "swarm_project_id?: Uuid",
            t.creator_user_id   AS "creator_user_id?: Uuid",
            t.assignee_user_id  AS "assignee_user_id?: Uuid",
            t.deleted_by_user_id AS "deleted_by_user_id?: Uuid",
            t.executing_node_id AS "executing_node_id?: Uuid",
            t.owner_node_id     AS "owner_node_id?: Uuid",
            t.owner_name        AS "owner_name?",
            t.source_task_id    AS "source_task_id?: Uuid",
            t.source_node_id    AS "source_node_id?: Uuid",
            t.title             AS "title!",
            t.description       AS "description?",
            t.status            AS "status!: TaskStatus",
            t.version           AS "version!",
            t.deleted_at        AS "deleted_at?",
            t.shared_at         AS "shared_at?",
            t.archived_at       AS "archived_at?",
            t.created_at        AS "created_at!",
            t.updated_at        AS "updated_at!",
            NULL::text          AS "assignee_name?",
            NULL::text          AS "assignee_username?",
            NULL::timestamptz   AS "activity_at?"
        "#,
            task_id,
            data.title,
            data.description,
            data.status as Option<TaskStatus>,
            data.version,
            should_update_archived,
            archived_at_value
        )
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| {
            SharedTaskError::Conflict(
                "Task update failed: version mismatch or task was deleted".to_string(),
            )
        })?;

        ensure_text_size(&task.title, task.description.as_deref())?;

        let user = match task.assignee_user_id {
            Some(user_id) => fetch_user(&mut tx, user_id).await?,
            None => None,
        };

        insert_activity(&mut tx, &task, user.as_ref(), "task.updated").await?;
        tx.commit().await.map_err(SharedTaskError::from)?;
        Ok(SharedTaskWithUser::new(task, user))
    }

    /// Assigns a task to a new assignee, updating the task's version and recording a `task.reassigned` activity.
    ///
    /// The update enforces optimistic concurrency using `data.version` and optionally verifies the previous
    /// assignee via `data.previous_assignee_user_id`. On success returns the updated task together with the
    /// optionally fetched assignee user. Commits the change as a single transaction.
    ///
    /// # Errors
    ///
    /// Returns `SharedTaskError::Conflict` if the provided version or previous assignee does not match the current row,
    /// or other `SharedTaskError` variants for database/serialization/identity failures.
    ///
    /// # Examples
    ///
    /// ```
    /// # async fn example(repo: &SharedTaskRepository<'_>, task_id: uuid::Uuid) -> Result<(), SharedTaskError> {
    /// let data = AssignTaskData {
    ///     new_assignee_user_id: Some(uuid::Uuid::new_v4()),
    ///     previous_assignee_user_id: None,
    ///     version: None,
    /// };
    /// let result = repo.assign_task(task_id, data).await?;
    /// println!("Assigned task {} to {:?}", result.task.id, result.user);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn assign_task(
        &self,
        task_id: Uuid,
        data: AssignTaskData,
    ) -> Result<SharedTaskWithUser, SharedTaskError> {
        let mut tx = self.pool.begin().await.map_err(SharedTaskError::from)?;

        let task = sqlx::query_as!(
            SharedTask,
            r#"
        UPDATE shared_tasks AS t
        SET assignee_user_id = $2,
            version = t.version + 1
        WHERE t.id = $1
          AND t.version = COALESCE($4, t.version)
          AND ($3::uuid IS NULL OR t.assignee_user_id = $3::uuid)
          AND t.deleted_at IS NULL
        RETURNING
            t.id                AS "id!",
            t.organization_id   AS "organization_id!: Uuid",
            t.project_id        AS "project_id?: Uuid",
            t.swarm_project_id  AS "swarm_project_id?: Uuid",
            t.creator_user_id   AS "creator_user_id?: Uuid",
            t.assignee_user_id  AS "assignee_user_id?: Uuid",
            t.deleted_by_user_id AS "deleted_by_user_id?: Uuid",
            t.executing_node_id AS "executing_node_id?: Uuid",
            t.owner_node_id     AS "owner_node_id?: Uuid",
            t.owner_name        AS "owner_name?",
            t.source_task_id    AS "source_task_id?: Uuid",
            t.source_node_id    AS "source_node_id?: Uuid",
            t.title             AS "title!",
            t.description       AS "description?",
            t.status            AS "status!: TaskStatus",
            t.version           AS "version!",
            t.deleted_at        AS "deleted_at?",
            t.shared_at         AS "shared_at?",
            t.archived_at       AS "archived_at?",
            t.created_at        AS "created_at!",
            t.updated_at        AS "updated_at!",
            NULL::text          AS "assignee_name?",
            NULL::text          AS "assignee_username?",
            NULL::timestamptz   AS "activity_at?"
        "#,
            task_id,
            data.new_assignee_user_id,
            data.previous_assignee_user_id,
            data.version
        )
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| {
            SharedTaskError::Conflict("task version or previous assignee mismatch".to_string())
        })?;

        let user = match data.new_assignee_user_id {
            Some(user_id) => fetch_user(&mut tx, user_id).await?,
            None => None,
        };

        insert_activity(&mut tx, &task, user.as_ref(), "task.reassigned").await?;
        tx.commit().await.map_err(SharedTaskError::from)?;
        Ok(SharedTaskWithUser::new(task, user))
    }

    /// Update a shared task's status based on a node execution report.
    ///
    /// This operation bypasses user validation and is intended for system-invoked node updates.
    ///
    /// # Returns
    ///
    /// The updated `SharedTask`.
    ///
    /// # Errors
    ///
    /// Returns `SharedTaskError::NotFound` if the task does not exist or has been deleted.
    ///
    /// # Examples
    ///
    /// ```
    /// # use uuid::Uuid;
    /// # use crate::db::tasks::{SharedTaskRepository, TaskStatus};
    /// # async fn example(repo: &SharedTaskRepository<'_>) {
    /// let task_id = Uuid::new_v4();
    /// let _updated = repo.update_status_from_node(task_id, TaskStatus::InProgress).await;
    /// # }
    /// ```
    pub async fn update_status_from_node(
        &self,
        task_id: Uuid,
        status: TaskStatus,
    ) -> Result<SharedTask, SharedTaskError> {
        let task = sqlx::query_as!(
            SharedTask,
            r#"
            UPDATE shared_tasks AS t
            SET status = $2,
                version = t.version + 1,
                updated_at = NOW()
            WHERE t.id = $1
              AND t.deleted_at IS NULL
            RETURNING
                t.id                AS "id!",
                t.organization_id   AS "organization_id!: Uuid",
                t.project_id        AS "project_id?: Uuid",
                t.swarm_project_id  AS "swarm_project_id?: Uuid",
                t.creator_user_id   AS "creator_user_id?: Uuid",
                t.assignee_user_id  AS "assignee_user_id?: Uuid",
                t.deleted_by_user_id AS "deleted_by_user_id?: Uuid",
                t.executing_node_id AS "executing_node_id?: Uuid",
                t.owner_node_id     AS "owner_node_id?: Uuid",
                t.owner_name        AS "owner_name?",
                t.source_task_id    AS "source_task_id?: Uuid",
                t.source_node_id    AS "source_node_id?: Uuid",
                t.title             AS "title!",
                t.description       AS "description?",
                t.status            AS "status!: TaskStatus",
                t.version           AS "version!",
                t.deleted_at        AS "deleted_at?",
                t.shared_at         AS "shared_at?",
                t.archived_at       AS "archived_at?",
                t.created_at        AS "created_at!",
                t.updated_at        AS "updated_at!",
                NULL::text          AS "assignee_name?",
                NULL::text          AS "assignee_username?",
                NULL::timestamptz   AS "activity_at?"
            "#,
            task_id,
            status as TaskStatus,
        )
        .fetch_optional(self.pool)
        .await?
        .ok_or(SharedTaskError::NotFound)?;

        Ok(task)
    }

    /// Set the executing node for a task.
    ///
    /// This is called when a task attempt is dispatched to a specific node.
    pub async fn set_executing_node(
        &self,
        task_id: Uuid,
        node_id: Option<Uuid>,
    ) -> Result<(), SharedTaskError> {
        sqlx::query!(
            r#"
            UPDATE shared_tasks
            SET executing_node_id = $2,
                updated_at = NOW()
            WHERE id = $1
              AND deleted_at IS NULL
            "#,
            task_id,
            node_id
        )
        .execute(self.pool)
        .await?;
        Ok(())
    }

    /// Marks a shared task as deleted, records who deleted it, appends a "task.deleted" activity, and returns the updated task record.
    ///
    /// The task's `deleted_at`, `deleted_by_user_id`, and `version` fields will be updated in the database. The function also inserts a corresponding activity row for the deletion event.
    ///
    /// # Returns
    ///
    /// `SharedTaskWithUser` containing the updated `SharedTask` and `None` for the user (assignee is not fetched here).
    ///
    /// # Errors
    ///
    /// Returns `SharedTaskError::Conflict` if the provided `version` does not match the current task version or the task was already deleted. Other `SharedTaskError` variants indicate database/serialization/identity failures.
    ///
    /// # Examples
    ///
    /// ```
    /// # use uuid::Uuid;
    /// # async fn example(repo: &crate::db::tasks::SharedTaskRepository<'_>) -> Result<(), crate::db::tasks::SharedTaskError> {
    /// let task_id = Uuid::new_v4();
    /// let data = crate::db::tasks::DeleteTaskData {
    ///     acting_user_id: Uuid::new_v4(),
    ///     version: Some(1),
    /// };
    /// let result = repo.delete_task(task_id, data).await?;
    /// assert_eq!(result.task.id, task_id);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn delete_task(
        &self,
        task_id: Uuid,
        data: DeleteTaskData,
    ) -> Result<SharedTaskWithUser, SharedTaskError> {
        let mut tx = self.pool.begin().await.map_err(SharedTaskError::from)?;

        let task = sqlx::query_as!(
            SharedTask,
            r#"
        UPDATE shared_tasks AS t
        SET deleted_at = NOW(),
            deleted_by_user_id = $3,
            version = t.version + 1
        WHERE t.id = $1
          AND t.version = COALESCE($2, t.version)
          AND t.deleted_at IS NULL
        RETURNING
            t.id                AS "id!",
            t.organization_id   AS "organization_id!: Uuid",
            t.project_id        AS "project_id?: Uuid",
            t.swarm_project_id  AS "swarm_project_id?: Uuid",
            t.creator_user_id   AS "creator_user_id?: Uuid",
            t.assignee_user_id  AS "assignee_user_id?: Uuid",
            t.deleted_by_user_id AS "deleted_by_user_id?: Uuid",
            t.executing_node_id AS "executing_node_id?: Uuid",
            t.owner_node_id     AS "owner_node_id?: Uuid",
            t.owner_name        AS "owner_name?",
            t.source_task_id    AS "source_task_id?: Uuid",
            t.source_node_id    AS "source_node_id?: Uuid",
            t.title             AS "title!",
            t.description       AS "description?",
            t.status            AS "status!: TaskStatus",
            t.version           AS "version!",
            t.deleted_at        AS "deleted_at?",
            t.shared_at         AS "shared_at?",
            t.archived_at       AS "archived_at?",
            t.created_at        AS "created_at!",
            t.updated_at        AS "updated_at!",
            NULL::text          AS "assignee_name?",
            NULL::text          AS "assignee_username?",
            NULL::timestamptz   AS "activity_at?"
        "#,
            task_id,
            data.version,
            data.acting_user_id
        )
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| {
            SharedTaskError::Conflict(
                "Task delete failed: version mismatch or task was already deleted".to_string(),
            )
        })?;

        insert_activity(&mut tx, &task, None, "task.deleted").await?;
        tx.commit().await.map_err(SharedTaskError::from)?;
        Ok(SharedTaskWithUser::new(task, None))
    }
}

pub(crate) fn ensure_text_size(
    title: &str,
    description: Option<&str>,
) -> Result<(), SharedTaskError> {
    let total = title.len() + description.map(|value| value.len()).unwrap_or(0);

    if total > MAX_SHARED_TASK_TEXT_BYTES {
        return Err(SharedTaskError::PayloadTooLarge);
    }

    Ok(())
}

async fn insert_activity(
    tx: &mut Tx<'_>,
    task: &SharedTask,
    user: Option<&UserData>,
    event_type: &str,
) -> Result<(), SharedTaskError> {
    let payload = SharedTaskActivityPayload {
        task: task.clone(),
        user: user.cloned(),
    };
    let payload = serde_json::to_value(payload).map_err(SharedTaskError::Serialization)?;

    // First attempt at inserting - if partitions are missing we retry after provisioning.
    match do_insert_activity(tx, task, event_type, payload.clone()).await {
        Ok(_) => Ok(()),
        Err(err) => {
            if let sqlx::Error::Database(db_err) = &err
                && maintenance::is_partition_missing_error(db_err.as_ref())
            {
                let code_owned = db_err.code().map(|c| c.to_string());
                let code = code_owned.as_deref().unwrap_or_default();
                tracing::warn!(
                    "Activity partition missing ({}), creating current and next partitions",
                    code
                );

                maintenance::ensure_future_partitions(tx.as_mut())
                    .await
                    .map_err(SharedTaskError::from)?;

                return do_insert_activity(tx, task, event_type, payload)
                    .await
                    .map_err(SharedTaskError::from);
            }

            Err(SharedTaskError::from(err))
        }
    }
}

async fn do_insert_activity(
    tx: &mut Tx<'_>,
    task: &SharedTask,
    event_type: &str,
    payload: serde_json::Value,
) -> Result<(), sqlx::Error> {
    // Use project_id if available, otherwise fall back to swarm_project_id
    // Skip activity insertion if neither is available (shouldn't happen in practice)
    let activity_project_id = match task.project_id.or(task.swarm_project_id) {
        Some(id) => id,
        None => {
            tracing::warn!(
                task_id = %task.id,
                "skipping activity insertion: task has no project_id or swarm_project_id"
            );
            return Ok(());
        }
    };

    sqlx::query!(
        r#"
        WITH next AS (
            INSERT INTO project_activity_counters AS counters (project_id, last_seq)
            VALUES ($1, 1)
            ON CONFLICT (project_id)
            DO UPDATE SET last_seq = counters.last_seq + 1
            RETURNING last_seq
        )
        INSERT INTO activity (
            project_id,
            seq,
            assignee_user_id,
            event_type,
            payload
        )
        SELECT $1, next.last_seq, $2, $3, $4
        FROM next
        "#,
        activity_project_id,
        task.assignee_user_id,
        event_type,
        payload
    )
    .execute(&mut **tx)
    .await
    .map(|_| ())
}

impl SharedTaskRepository<'_> {
    pub async fn organization_id(
        pool: &PgPool,
        task_id: Uuid,
    ) -> Result<Option<Uuid>, sqlx::Error> {
        sqlx::query_scalar!(
            r#"
            SELECT organization_id
            FROM shared_tasks
            WHERE id = $1
            "#,
            task_id
        )
        .fetch_optional(pool)
        .await
    }
}
