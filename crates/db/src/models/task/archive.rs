//! Archiving operations for tasks.

use chrono::{DateTime, Utc};
use sqlx::{QueryBuilder, Sqlite, SqlitePool};
use uuid::Uuid;

use super::{Task, TaskStatus};

impl Task {
    /// Archive a task by setting archived_at to the current timestamp.
    /// Returns the updated task.
    pub async fn archive(pool: &SqlitePool, id: Uuid) -> Result<Self, sqlx::Error> {
        sqlx::query_as!(
            Task,
            r#"UPDATE tasks
               SET archived_at = datetime('now', 'subsec'), updated_at = datetime('now', 'subsec')
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

    /// Unarchive a task by setting archived_at to NULL.
    /// Returns the updated task.
    pub async fn unarchive(pool: &SqlitePool, id: Uuid) -> Result<Self, sqlx::Error> {
        sqlx::query_as!(
            Task,
            r#"UPDATE tasks
               SET archived_at = NULL, updated_at = datetime('now', 'subsec')
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

    /// Archive multiple tasks by their IDs.
    /// Returns the number of tasks archived.
    ///
    /// Uses a single bulk UPDATE query with IN clause for O(1) database calls
    /// instead of O(n) individual queries.
    pub async fn archive_many(pool: &SqlitePool, ids: &[Uuid]) -> Result<u64, sqlx::Error> {
        if ids.is_empty() {
            return Ok(0);
        }

        let mut builder = QueryBuilder::<Sqlite>::new(
            "UPDATE tasks SET archived_at = datetime('now', 'subsec'), updated_at = datetime('now', 'subsec') WHERE id IN (",
        );
        {
            let mut separated = builder.separated(", ");
            for id in ids {
                separated.push_bind(id);
            }
        }
        builder.push(")");
        let result = builder.build().execute(pool).await?;
        Ok(result.rows_affected())
    }

    /// Auto-unarchive a task if it's currently archived.
    /// Returns true if the task was unarchived, false if it was already active.
    /// This is useful for auto-unarchiving tasks when they receive activity
    /// (edits, new attempts, subtasks, or follow-up prompts).
    pub async fn unarchive_if_archived(pool: &SqlitePool, id: Uuid) -> Result<bool, sqlx::Error> {
        let result = sqlx::query!(
            "UPDATE tasks SET archived_at = NULL, updated_at = CURRENT_TIMESTAMP WHERE id = $1 AND archived_at IS NOT NULL",
            id
        )
        .execute(pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{
        project::{CreateProject, Project},
        task::{tests::setup_test_pool, CreateTask},
    };

    #[tokio::test]
    async fn test_archive_unarchive() {
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
        assert!(task.archived_at.is_none());

        // Archive
        let archived = Task::archive(&pool, task_id)
            .await
            .expect("Archive failed");
        assert!(archived.archived_at.is_some());

        // Unarchive
        let unarchived = Task::unarchive(&pool, task_id)
            .await
            .expect("Unarchive failed");
        assert!(unarchived.archived_at.is_none());
    }

    #[tokio::test]
    async fn test_archive_many() {
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

        // Create tasks
        let mut task_ids = Vec::new();
        for i in 0..5 {
            let task_id = Uuid::new_v4();
            let task_data = CreateTask::from_title_description(
                project_id,
                format!("Task {}", i),
                None,
            );
            Task::create(&pool, &task_data, task_id)
                .await
                .expect("Failed to create task");
            task_ids.push(task_id);
        }

        // Archive multiple
        let archived_count = Task::archive_many(&pool, &task_ids)
            .await
            .expect("Archive many failed");
        assert_eq!(archived_count, 5);

        // Verify all are archived
        for task_id in &task_ids {
            let task = Task::find_by_id(&pool, *task_id)
                .await
                .expect("Query failed")
                .expect("Task not found");
            assert!(task.archived_at.is_some());
        }
    }

    #[tokio::test]
    async fn test_archive_many_empty() {
        let (pool, _temp_dir) = setup_test_pool().await;

        // Archive empty list should return 0
        let archived_count = Task::archive_many(&pool, &[])
            .await
            .expect("Archive many failed");
        assert_eq!(archived_count, 0);
    }

    #[tokio::test]
    async fn test_unarchive_if_archived() {
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

        // Create and archive task
        let task_id = Uuid::new_v4();
        let task_data =
            CreateTask::from_title_description(project_id, "Test Task".to_string(), None);
        Task::create(&pool, &task_data, task_id)
            .await
            .expect("Failed to create task");
        Task::archive(&pool, task_id)
            .await
            .expect("Archive failed");

        // Unarchive if archived - should return true
        let was_unarchived = Task::unarchive_if_archived(&pool, task_id)
            .await
            .expect("Unarchive failed");
        assert!(was_unarchived);

        // Call again - should return false (already unarchived)
        let was_unarchived = Task::unarchive_if_archived(&pool, task_id)
            .await
            .expect("Unarchive failed");
        assert!(!was_unarchived);
    }
}
