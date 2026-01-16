//! Cleanup operations for archived tasks.
//!
//! Functions for counting, finding, and deleting archived tasks in terminal states
//! (done/cancelled). This enables users to purge old completed tasks to reclaim space.

use chrono::{Duration, Utc};
use sqlx::SqlitePool;

use super::{Task, TaskStatus};

impl Task {
    /// Count archived tasks in terminal states (done/cancelled) older than the specified days.
    /// Returns the count of tasks that would be purged.
    pub async fn count_archived_terminal_older_than(
        pool: &SqlitePool,
        days: i64,
    ) -> Result<i64, sqlx::Error> {
        let cutoff = Utc::now() - Duration::days(days);
        let result = sqlx::query_scalar!(
            r#"SELECT COUNT(*) as "count!: i64" FROM tasks
               WHERE archived_at IS NOT NULL
               AND archived_at < ?
               AND status IN ('done', 'cancelled')"#,
            cutoff
        )
        .fetch_one(pool)
        .await?;
        Ok(result)
    }

    /// Delete archived tasks in terminal states (done/cancelled) older than the specified days.
    /// Returns the number of tasks deleted.
    ///
    /// CASCADE delete will automatically remove related:
    /// - task_attempts
    /// - execution_processes (via task_attempts)
    /// - log_entries (via execution_processes)
    pub async fn delete_archived_terminal_older_than(
        pool: &SqlitePool,
        days: i64,
    ) -> Result<i64, sqlx::Error> {
        let cutoff = Utc::now() - Duration::days(days);
        let result = sqlx::query!(
            r#"DELETE FROM tasks
               WHERE archived_at IS NOT NULL
               AND archived_at < ?
               AND status IN ('done', 'cancelled')"#,
            cutoff
        )
        .execute(pool)
        .await?;
        Ok(result.rows_affected() as i64)
    }

    /// Find archived tasks that are NOT in terminal states (done/cancelled).
    /// These are "stuck" tasks that were archived but not completed.
    /// Users may want to review these and either complete or unarchive them.
    pub async fn find_archived_non_terminal(pool: &SqlitePool) -> Result<Vec<Task>, sqlx::Error> {
        sqlx::query_as!(
            Task,
            r#"SELECT
                id as "id!: uuid::Uuid",
                project_id as "project_id!: uuid::Uuid",
                title,
                description,
                status as "status!: TaskStatus",
                parent_task_id as "parent_task_id: uuid::Uuid",
                shared_task_id as "shared_task_id: uuid::Uuid",
                created_at as "created_at!: chrono::DateTime<chrono::Utc>",
                updated_at as "updated_at!: chrono::DateTime<chrono::Utc>",
                remote_assignee_user_id as "remote_assignee_user_id: uuid::Uuid",
                remote_assignee_name,
                remote_assignee_username,
                remote_version as "remote_version!: i64",
                remote_last_synced_at as "remote_last_synced_at: chrono::DateTime<chrono::Utc>",
                remote_stream_node_id as "remote_stream_node_id: uuid::Uuid",
                remote_stream_url,
                archived_at as "archived_at: chrono::DateTime<chrono::Utc>",
                activity_at as "activity_at: chrono::DateTime<chrono::Utc>"
            FROM tasks
            WHERE archived_at IS NOT NULL
            AND status NOT IN ('done', 'cancelled')"#
        )
        .fetch_all(pool)
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{
        project::{CreateProject, Project},
        task::{tests::setup_test_pool, CreateTask},
    };
    use uuid::Uuid;

    #[tokio::test]
    async fn test_count_archived_terminal_older_than() {
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
        Project::create(&pool, &project_data, project_id)
            .await
            .expect("Failed to create project");

        // Create and archive a done task
        let task_id = Uuid::new_v4();
        let task_data =
            CreateTask::from_title_description(project_id, "Done Task".to_string(), None);
        Task::create(&pool, &task_data, task_id)
            .await
            .expect("Failed to create task");
        Task::update_status(&pool, task_id, TaskStatus::Done)
            .await
            .expect("Failed to update status");
        Task::archive(&pool, task_id)
            .await
            .expect("Failed to archive task");

        // With 1 day cutoff, task archived just now should NOT be counted
        // (it's less than 1 day old)
        let count = Task::count_archived_terminal_older_than(&pool, 1)
            .await
            .expect("Failed to count");
        assert_eq!(count, 0);

        // With 0 days cutoff (cutoff = now), task archived just before should be counted
        // because archived_at < now (even if just by milliseconds)
        let count = Task::count_archived_terminal_older_than(&pool, 0)
            .await
            .expect("Failed to count");
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn test_delete_archived_terminal_older_than() {
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
        Project::create(&pool, &project_data, project_id)
            .await
            .expect("Failed to create project");

        // Create and archive a cancelled task
        let task_id = Uuid::new_v4();
        let task_data =
            CreateTask::from_title_description(project_id, "Cancelled Task".to_string(), None);
        Task::create(&pool, &task_data, task_id)
            .await
            .expect("Failed to create task");
        Task::update_status(&pool, task_id, TaskStatus::Cancelled)
            .await
            .expect("Failed to update status");
        Task::archive(&pool, task_id)
            .await
            .expect("Failed to archive task");

        // Delete with 0 days cutoff (cutoff = now) to catch freshly archived task
        let deleted = Task::delete_archived_terminal_older_than(&pool, 0)
            .await
            .expect("Failed to delete");
        assert_eq!(deleted, 1);

        // Verify task is gone
        let task = Task::find_by_id(&pool, task_id)
            .await
            .expect("Query failed");
        assert!(task.is_none());
    }

    #[tokio::test]
    async fn test_find_archived_non_terminal() {
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
        Project::create(&pool, &project_data, project_id)
            .await
            .expect("Failed to create project");

        // Create task in "todo" status (non-terminal) and archive it
        let task_id = Uuid::new_v4();
        let task_data =
            CreateTask::from_title_description(project_id, "Stuck Task".to_string(), None);
        Task::create(&pool, &task_data, task_id)
            .await
            .expect("Failed to create task");
        // Archive without changing status - this creates a "stuck" archived task
        Task::archive(&pool, task_id)
            .await
            .expect("Failed to archive task");

        // Find non-terminal archived tasks
        let stuck_tasks = Task::find_archived_non_terminal(&pool)
            .await
            .expect("Failed to find stuck tasks");
        assert_eq!(stuck_tasks.len(), 1);
        assert_eq!(stuck_tasks[0].id, task_id);
        assert_eq!(stuck_tasks[0].status, TaskStatus::Todo);
    }

    #[tokio::test]
    async fn test_terminal_archived_not_in_find_non_terminal() {
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
        Project::create(&pool, &project_data, project_id)
            .await
            .expect("Failed to create project");

        // Create task in "done" status (terminal) and archive it
        let task_id = Uuid::new_v4();
        let task_data =
            CreateTask::from_title_description(project_id, "Completed Task".to_string(), None);
        Task::create(&pool, &task_data, task_id)
            .await
            .expect("Failed to create task");
        Task::update_status(&pool, task_id, TaskStatus::Done)
            .await
            .expect("Failed to update status");
        Task::archive(&pool, task_id)
            .await
            .expect("Failed to archive task");

        // Find non-terminal archived tasks - should be empty
        let stuck_tasks = Task::find_archived_non_terminal(&pool)
            .await
            .expect("Failed to find stuck tasks");
        assert!(stuck_tasks.is_empty());
    }
}
