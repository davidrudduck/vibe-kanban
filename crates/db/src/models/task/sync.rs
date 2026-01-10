//! Hive sync operations for tasks.
//!
//! These operations handle synchronization between local tasks and the Hive (remote server).

use chrono::{DateTime, Utc};
use sqlx::{Executor, Sqlite, SqlitePool};
use uuid::Uuid;

use super::{SyncTask, Task, TaskStatus};

impl Task {
    /// Sync a task from a shared task.
    /// Creates a new task if it doesn't exist, or updates if it does.
    pub async fn sync_from_shared_task<'e, E>(
        executor: E,
        data: SyncTask,
        create_if_not_exists: bool,
    ) -> Result<bool, sqlx::Error>
    where
        E: Executor<'e, Database = Sqlite>,
    {
        let new_task_id = Uuid::new_v4();

        let result = sqlx::query!(
            r#"
            INSERT INTO tasks (
                id,
                project_id,
                title,
                description,
                status,
                shared_task_id,
                activity_at
            )
            SELECT
                $1,
                $2,
                $3,
                $4,
                $5,
                $6,
                $7
            WHERE $8
               OR EXISTS (
                    SELECT 1 FROM tasks WHERE shared_task_id = $6
               )
            ON CONFLICT(shared_task_id) WHERE shared_task_id IS NOT NULL DO UPDATE SET
                project_id = excluded.project_id,
                title = excluded.title,
                description = excluded.description,
                status = excluded.status,
                activity_at = excluded.activity_at,
                updated_at = datetime('now', 'subsec')
            "#,
            new_task_id,
            data.project_id,
            data.title,
            data.description,
            data.status,
            data.shared_task_id,
            data.activity_at,
            create_if_not_exists
        )
        .execute(executor)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Set the shared_task_id for a task.
    pub async fn set_shared_task_id<'e, E>(
        executor: E,
        id: Uuid,
        shared_task_id: Option<Uuid>,
    ) -> Result<(), sqlx::Error>
    where
        E: Executor<'e, Database = Sqlite>,
    {
        sqlx::query!(
            "UPDATE tasks SET shared_task_id = $2, updated_at = CURRENT_TIMESTAMP WHERE id = $1",
            id,
            shared_task_id
        )
        .execute(executor)
        .await?;
        Ok(())
    }

    /// Updates the shared_task_id for a task and returns the updated task.
    ///
    /// This is used during re-sync when a task needs to be re-linked to the Hive.
    pub async fn update_shared_task_id(
        pool: &SqlitePool,
        id: Uuid,
        shared_task_id: Uuid,
    ) -> Result<Self, sqlx::Error> {
        sqlx::query_as!(
            Self,
            r#"UPDATE tasks
               SET shared_task_id = $2, updated_at = CURRENT_TIMESTAMP
               WHERE id = $1
               RETURNING id as "id!: Uuid", project_id as "project_id!: Uuid", title, description, status as "status!: TaskStatus", parent_task_id as "parent_task_id: Uuid", shared_task_id as "shared_task_id: Uuid", created_at as "created_at!: DateTime<Utc>", updated_at as "updated_at!: DateTime<Utc>",
                         remote_assignee_user_id as "remote_assignee_user_id: Uuid",
                         remote_assignee_name,
                         remote_assignee_username,
                         remote_version as "remote_version!: i64",
                         remote_last_synced_at as "remote_last_synced_at: DateTime<Utc>",
                         remote_stream_node_id as "remote_stream_node_id: Uuid",
                         remote_stream_url,
                         archived_at as "archived_at: DateTime<Utc>",
                         activity_at as "activity_at: DateTime<Utc>""#,
            id,
            shared_task_id
        )
        .fetch_one(pool)
        .await
    }

    /// Clears the shared_task_id for a task and resets remote_version.
    ///
    /// This is used when the node cannot resync a task to the Hive (e.g., node
    /// is not connected) and needs to treat the task as local-only.
    pub async fn clear_shared_task_id(pool: &SqlitePool, id: Uuid) -> Result<Self, sqlx::Error> {
        sqlx::query_as!(
            Self,
            r#"UPDATE tasks
               SET shared_task_id = NULL, remote_version = 0, updated_at = CURRENT_TIMESTAMP
               WHERE id = $1
               RETURNING id as "id!: Uuid", project_id as "project_id!: Uuid", title, description, status as "status!: TaskStatus", parent_task_id as "parent_task_id: Uuid", shared_task_id as "shared_task_id: Uuid", created_at as "created_at!: DateTime<Utc>", updated_at as "updated_at!: DateTime<Utc>",
                         remote_assignee_user_id as "remote_assignee_user_id: Uuid",
                         remote_assignee_name,
                         remote_assignee_username,
                         remote_version as "remote_version!: i64",
                         remote_last_synced_at as "remote_last_synced_at: DateTime<Utc>",
                         remote_stream_node_id as "remote_stream_node_id: Uuid",
                         remote_stream_url,
                         archived_at as "archived_at: DateTime<Utc>",
                         activity_at as "activity_at: DateTime<Utc>""#,
            id
        )
        .fetch_one(pool)
        .await
    }

    /// Clear shared_task_id for all tasks belonging to a project with the given remote_project_id.
    /// This breaks the link between local tasks and hive tasks when a project is unlinked.
    pub async fn clear_shared_task_ids_for_remote_project<'e, E>(
        executor: E,
        remote_project_id: Uuid,
    ) -> Result<u64, sqlx::Error>
    where
        E: Executor<'e, Database = Sqlite>,
    {
        let result = sqlx::query!(
            r#"UPDATE tasks
               SET shared_task_id = NULL
               WHERE project_id IN (
                   SELECT id FROM projects WHERE remote_project_id = $1
               )"#,
            remote_project_id
        )
        .execute(executor)
        .await?;
        Ok(result.rows_affected())
    }

    /// Upsert a remote task from the Hive.
    #[allow(clippy::too_many_arguments)]
    pub async fn upsert_remote_task<'e, E>(
        executor: E,
        local_id: Uuid,
        project_id: Uuid,
        shared_task_id: Uuid,
        title: String,
        description: Option<String>,
        status: TaskStatus,
        remote_assignee_user_id: Option<Uuid>,
        remote_assignee_name: Option<String>,
        remote_assignee_username: Option<String>,
        remote_version: i64,
        activity_at: Option<DateTime<Utc>>,
        archived_at: Option<DateTime<Utc>>,
    ) -> Result<Self, sqlx::Error>
    where
        E: Executor<'e, Database = Sqlite>,
    {
        let now = Utc::now();
        sqlx::query_as!(
            Task,
            r#"INSERT INTO tasks (
                    id,
                    project_id,
                    title,
                    description,
                    status,
                    shared_task_id,
                    remote_assignee_user_id,
                    remote_assignee_name,
                    remote_assignee_username,
                    remote_version,
                    remote_last_synced_at,
                    activity_at,
                    archived_at
                ) VALUES (
                    $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13
                )
                ON CONFLICT(shared_task_id) WHERE shared_task_id IS NOT NULL DO UPDATE SET
                    title = excluded.title,
                    description = excluded.description,
                    status = excluded.status,
                    remote_assignee_user_id = excluded.remote_assignee_user_id,
                    remote_assignee_name = excluded.remote_assignee_name,
                    remote_assignee_username = excluded.remote_assignee_username,
                    remote_version = excluded.remote_version,
                    remote_last_synced_at = excluded.remote_last_synced_at,
                    activity_at = excluded.activity_at,
                    archived_at = excluded.archived_at,
                    updated_at = datetime('now', 'subsec')
                RETURNING id as "id!: Uuid", project_id as "project_id!: Uuid", title, description, status as "status!: TaskStatus", parent_task_id as "parent_task_id: Uuid", shared_task_id as "shared_task_id: Uuid", created_at as "created_at!: DateTime<Utc>", updated_at as "updated_at!: DateTime<Utc>",
                          remote_assignee_user_id as "remote_assignee_user_id: Uuid",
                          remote_assignee_name,
                          remote_assignee_username,
                          remote_version as "remote_version!: i64",
                          remote_last_synced_at as "remote_last_synced_at: DateTime<Utc>",
                          remote_stream_node_id as "remote_stream_node_id: Uuid",
                          remote_stream_url,
                          archived_at as "archived_at: DateTime<Utc>",
                          activity_at as "activity_at: DateTime<Utc>""#,
            local_id,
            project_id,
            title,
            description,
            status,
            shared_task_id,
            remote_assignee_user_id,
            remote_assignee_name,
            remote_assignee_username,
            remote_version,
            now,
            activity_at,
            archived_at
        )
        .fetch_one(executor)
        .await
    }

    /// Update remote stream location for a task.
    pub async fn set_remote_stream_location(
        pool: &SqlitePool,
        id: Uuid,
        stream_node_id: Option<Uuid>,
        stream_url: Option<String>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"UPDATE tasks
               SET remote_stream_node_id = $2,
                   remote_stream_url = $3,
                   updated_at = CURRENT_TIMESTAMP
               WHERE id = $1"#,
            id,
            stream_node_id,
            stream_url
        )
        .execute(pool)
        .await?;
        Ok(())
    }

    /// Delete a task by its shared_task_id.
    ///
    /// Used when syncing remote tasks and a task has been deleted on the Hive.
    pub async fn delete_by_shared_task_id<'e, E>(
        executor: E,
        shared_task_id: Uuid,
    ) -> Result<(), sqlx::Error>
    where
        E: Executor<'e, Database = Sqlite>,
    {
        sqlx::query!("DELETE FROM tasks WHERE shared_task_id = ?", shared_task_id)
            .execute(executor)
            .await?;
        Ok(())
    }

    /// Delete stale shared tasks for a project.
    ///
    /// Deletes tasks that have a shared_task_id but are not in the provided list
    /// of active shared task IDs. This is used during Electric sync to clean up
    /// tasks that have been deleted on the Hive.
    ///
    /// Only deletes tasks with a shared_task_id (synced from Hive), leaving
    /// locally-created tasks untouched.
    pub async fn delete_stale_shared_tasks(
        pool: &SqlitePool,
        project_id: Uuid,
        active_shared_task_ids: &[Uuid],
    ) -> Result<u64, sqlx::Error> {
        if active_shared_task_ids.is_empty() {
            return Ok(0);
        }

        // Build placeholders for the IN clause
        let placeholders: Vec<String> = active_shared_task_ids
            .iter()
            .enumerate()
            .map(|(i, _)| format!("${}", i + 2))
            .collect();
        let placeholders_str = placeholders.join(", ");

        let query = format!(
            r#"DELETE FROM tasks
               WHERE project_id = $1
               AND shared_task_id IS NOT NULL
               AND shared_task_id NOT IN ({})"#,
            placeholders_str
        );

        let mut query_builder = sqlx::query(&query).bind(project_id);
        for id in active_shared_task_ids {
            query_builder = query_builder.bind(id);
        }

        let result = query_builder.execute(pool).await?;
        Ok(result.rows_affected())
    }

    /// Clear shared_task_id for orphaned tasks.
    ///
    /// An orphaned task is one that has a shared_task_id but belongs to a project
    /// that is not linked to the Hive (remote_project_id IS NULL).
    /// This can happen if a project was previously linked but then unlinked.
    ///
    /// Returns the number of tasks that had their shared_task_id cleared.
    pub async fn clear_orphaned_shared_task_ids(pool: &SqlitePool) -> Result<u64, sqlx::Error> {
        let result = sqlx::query(
            r#"UPDATE tasks
               SET shared_task_id = NULL, updated_at = CURRENT_TIMESTAMP
               WHERE shared_task_id IS NOT NULL
               AND project_id IN (
                   SELECT id FROM projects WHERE remote_project_id IS NULL AND is_remote = 0
               )"#,
        )
        .execute(pool)
        .await?;

        Ok(result.rows_affected())
    }

    /// Find tasks that need to be synced to the Hive.
    ///
    /// A task needs syncing if:
    /// 1. It has no `shared_task_id` (not yet synced to Hive)
    /// 2. It has task attempts with no `hive_synced_at` (unsynced attempts)
    /// 3. Its project has a `remote_project_id` (project is linked to Hive)
    ///
    /// This query ensures we sync tasks before their attempts, so the attempts
    /// can reference a valid `shared_task_id`.
    pub async fn find_needing_sync(
        pool: &SqlitePool,
        limit: i64,
    ) -> Result<Vec<Self>, sqlx::Error> {
        // Use runtime query to avoid sqlx cache issues
        sqlx::query_as::<_, Self>(
            r#"SELECT DISTINCT
                t.id,
                t.project_id,
                t.title,
                t.description,
                t.status,
                t.parent_task_id,
                t.shared_task_id,
                t.created_at,
                t.updated_at,
                t.remote_assignee_user_id,
                t.remote_assignee_name,
                t.remote_assignee_username,
                t.remote_version,
                t.remote_last_synced_at,
                t.remote_stream_node_id,
                t.remote_stream_url,
                t.archived_at,
                t.activity_at
            FROM tasks t
            INNER JOIN task_attempts ta ON ta.task_id = t.id
            INNER JOIN projects p ON p.id = t.project_id
            WHERE t.shared_task_id IS NULL
              AND p.remote_project_id IS NOT NULL
              AND ta.hive_synced_at IS NULL
            ORDER BY t.created_at ASC
            LIMIT ?"#,
        )
        .bind(limit)
        .fetch_all(pool)
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{
        project::{CreateProject, Project},
        task::{CreateTask, tests::setup_test_pool},
    };

    #[tokio::test]
    async fn test_shared_task_id_operations() {
        let (pool, _temp_dir) = setup_test_pool().await;

        // Create project
        let project_id = Uuid::new_v4();
        let project_data = CreateProject {
            name: "Test Project".to_string(),
            git_repo_path: format!("/tmp/test-repo-{}", project_id),
            use_existing_repo: true,
            clone_url: None,
            setup_script: None,
            dev_script: None,
            cleanup_script: None,
            copy_files: None,
        };
        let _project = Project::create(&pool, &project_data, project_id)
            .await
            .expect("Failed to create project");

        // Create task
        let task_id = Uuid::new_v4();
        let task_data =
            CreateTask::from_title_description(project_id, "Test Task".to_string(), None);
        let task = Task::create(&pool, &task_data, task_id)
            .await
            .expect("Failed to create task");
        assert!(task.shared_task_id.is_none());

        // Set shared_task_id
        let shared_task_id = Uuid::new_v4();
        Task::set_shared_task_id(&pool, task_id, Some(shared_task_id))
            .await
            .expect("Set failed");

        let updated = Task::find_by_id(&pool, task_id)
            .await
            .expect("Query failed")
            .expect("Task not found");
        assert_eq!(updated.shared_task_id, Some(shared_task_id));

        // Update shared_task_id
        let new_shared_task_id = Uuid::new_v4();
        let updated = Task::update_shared_task_id(&pool, task_id, new_shared_task_id)
            .await
            .expect("Update failed");
        assert_eq!(updated.shared_task_id, Some(new_shared_task_id));

        // Clear shared_task_id
        let cleared = Task::clear_shared_task_id(&pool, task_id)
            .await
            .expect("Clear failed");
        assert!(cleared.shared_task_id.is_none());
        assert_eq!(cleared.remote_version, 0);
    }

    #[tokio::test]
    async fn test_upsert_remote_task() {
        let (pool, _temp_dir) = setup_test_pool().await;

        // Create project
        let project_id = Uuid::new_v4();
        let project_data = CreateProject {
            name: "Test Project".to_string(),
            git_repo_path: format!("/tmp/test-repo-{}", project_id),
            use_existing_repo: true,
            clone_url: None,
            setup_script: None,
            dev_script: None,
            cleanup_script: None,
            copy_files: None,
        };
        let _project = Project::create(&pool, &project_data, project_id)
            .await
            .expect("Failed to create project");

        let local_id = Uuid::new_v4();
        let shared_task_id = Uuid::new_v4();

        // Insert new remote task
        let task = Task::upsert_remote_task(
            &pool,
            local_id,
            project_id,
            shared_task_id,
            "Remote Task".to_string(),
            Some("Description".to_string()),
            TaskStatus::Todo,
            None,
            None,
            None,
            1,
            None,
            None,
        )
        .await
        .expect("Upsert failed");

        assert_eq!(task.title, "Remote Task");
        assert_eq!(task.shared_task_id, Some(shared_task_id));
        assert_eq!(task.remote_version, 1);

        // Update existing remote task
        let updated = Task::upsert_remote_task(
            &pool,
            Uuid::new_v4(), // Different local_id doesn't matter for upsert
            project_id,
            shared_task_id,
            "Updated Remote Task".to_string(),
            None,
            TaskStatus::InProgress,
            None,
            None,
            None,
            2,
            None,
            None,
        )
        .await
        .expect("Upsert failed");

        // Should update existing record (same id)
        assert_eq!(updated.id, task.id);
        assert_eq!(updated.title, "Updated Remote Task");
        assert_eq!(updated.status, TaskStatus::InProgress);
        assert_eq!(updated.remote_version, 2);
    }

    #[tokio::test]
    async fn test_set_remote_stream_location() {
        let (pool, _temp_dir) = setup_test_pool().await;

        // Create project
        let project_id = Uuid::new_v4();
        let project_data = CreateProject {
            name: "Test Project".to_string(),
            git_repo_path: format!("/tmp/test-repo-{}", project_id),
            use_existing_repo: true,
            clone_url: None,
            setup_script: None,
            dev_script: None,
            cleanup_script: None,
            copy_files: None,
        };
        let _project = Project::create(&pool, &project_data, project_id)
            .await
            .expect("Failed to create project");

        // Create task
        let task_id = Uuid::new_v4();
        let task_data =
            CreateTask::from_title_description(project_id, "Test Task".to_string(), None);
        let task = Task::create(&pool, &task_data, task_id)
            .await
            .expect("Failed to create task");
        assert!(task.remote_stream_node_id.is_none());
        assert!(task.remote_stream_url.is_none());

        // Set stream location
        let node_id = Uuid::new_v4();
        Task::set_remote_stream_location(
            &pool,
            task_id,
            Some(node_id),
            Some("https://example.com/stream".to_string()),
        )
        .await
        .expect("Set failed");

        let updated = Task::find_by_id(&pool, task_id)
            .await
            .expect("Query failed")
            .expect("Task not found");
        assert_eq!(updated.remote_stream_node_id, Some(node_id));
        assert_eq!(
            updated.remote_stream_url,
            Some("https://example.com/stream".to_string())
        );
    }

    #[tokio::test]
    async fn test_delete_by_shared_task_id() {
        let (pool, _temp_dir) = setup_test_pool().await;

        // Create project
        let project_id = Uuid::new_v4();
        let project_data = CreateProject {
            name: "Test Project".to_string(),
            git_repo_path: format!("/tmp/test-repo-{}", project_id),
            use_existing_repo: true,
            clone_url: None,
            setup_script: None,
            dev_script: None,
            cleanup_script: None,
            copy_files: None,
        };
        let _project = Project::create(&pool, &project_data, project_id)
            .await
            .expect("Failed to create project");

        // Create task with shared_task_id
        let shared_task_id = Uuid::new_v4();
        let task_id = Uuid::new_v4();
        let mut task_data =
            CreateTask::from_title_description(project_id, "Test Task".to_string(), None);
        task_data.shared_task_id = Some(shared_task_id);
        Task::create(&pool, &task_data, task_id)
            .await
            .expect("Failed to create task");

        // Verify task exists
        let found = Task::find_by_shared_task_id(&pool, shared_task_id)
            .await
            .expect("Query failed");
        assert!(found.is_some());

        // Delete by shared_task_id
        Task::delete_by_shared_task_id(&pool, shared_task_id)
            .await
            .expect("Delete failed");

        // Verify task is gone
        let found = Task::find_by_shared_task_id(&pool, shared_task_id)
            .await
            .expect("Query failed");
        assert!(found.is_none());
    }
}
