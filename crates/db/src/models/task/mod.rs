//! Task model for managing tasks within projects.
//!
//! A task represents a unit of work within a project. Tasks can have parent-child
//! relationships, be archived, and sync with the Hive (remote server).

mod archive;
mod hierarchy;
mod queries;
mod sync;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Type};
use strum_macros::{Display, EnumString};
use ts_rs::TS;
use uuid::Uuid;

use super::task_attempt::TaskAttempt;

#[derive(
    Debug, Clone, Type, Serialize, Deserialize, PartialEq, TS, EnumString, Display, Default,
)]
#[sqlx(type_name = "task_status", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "kebab_case")]
pub enum TaskStatus {
    #[default]
    Todo,
    InProgress,
    InReview,
    Done,
    Cancelled,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, TS)]
pub struct Task {
    pub id: Uuid,
    pub project_id: Uuid, // Foreign key to Project
    pub title: String,
    pub description: Option<String>,
    pub status: TaskStatus,
    pub parent_task_id: Option<Uuid>, // Foreign key to parent Task
    pub shared_task_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    // Remote task fields - for task execution/streaming info
    pub remote_assignee_user_id: Option<Uuid>,
    pub remote_assignee_name: Option<String>,
    pub remote_assignee_username: Option<String>,
    pub remote_version: i64,
    pub remote_last_synced_at: Option<DateTime<Utc>>,
    pub remote_stream_node_id: Option<Uuid>,
    pub remote_stream_url: Option<String>,
    /// Timestamp when task was archived. NULL means not archived.
    #[ts(type = "Date | null")]
    pub archived_at: Option<DateTime<Utc>>,
    /// Timestamp of last significant activity (status change, execution start).
    /// Unlike updated_at, this is NOT updated for metadata changes like title/description edits.
    #[ts(type = "Date | null")]
    pub activity_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct TaskWithAttemptStatus {
    #[serde(flatten)]
    #[ts(flatten)]
    pub task: Task,
    pub has_in_progress_attempt: bool,
    pub has_merged_attempt: bool,
    pub last_attempt_failed: bool,
    pub executor: String,
    /// Latest execution start timestamp for sorting (codingagent only, non-dropped)
    #[ts(type = "Date | null")]
    pub latest_execution_started_at: Option<DateTime<Utc>>,
    /// Latest execution completion timestamp for sorting (codingagent only, non-dropped)
    #[ts(type = "Date | null")]
    pub latest_execution_completed_at: Option<DateTime<Utc>>,
}

impl std::ops::Deref for TaskWithAttemptStatus {
    type Target = Task;
    fn deref(&self) -> &Self::Target {
        &self.task
    }
}

impl std::ops::DerefMut for TaskWithAttemptStatus {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.task
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct TaskRelationships {
    pub parent_task: Option<Task>,    // The task that owns this attempt
    pub current_attempt: TaskAttempt, // The attempt we're viewing
    pub children: Vec<Task>,          // Tasks created by this attempt
}

#[derive(Debug, Serialize, Deserialize, TS)]
pub struct CreateTask {
    pub project_id: Uuid,
    pub title: String,
    pub description: Option<String>,
    pub status: Option<TaskStatus>,
    pub parent_task_id: Option<Uuid>,
    pub image_ids: Option<Vec<Uuid>>,
    pub shared_task_id: Option<Uuid>,
}

impl CreateTask {
    pub fn from_title_description(
        project_id: Uuid,
        title: String,
        description: Option<String>,
    ) -> Self {
        Self {
            project_id,
            title,
            description,
            status: Some(TaskStatus::Todo),
            parent_task_id: None,
            image_ids: None,
            shared_task_id: None,
        }
    }

    pub fn from_shared_task(
        project_id: Uuid,
        title: String,
        description: Option<String>,
        status: TaskStatus,
        shared_task_id: Uuid,
    ) -> Self {
        Self {
            project_id,
            title,
            description,
            status: Some(status),
            parent_task_id: None,
            image_ids: None,
            shared_task_id: Some(shared_task_id),
        }
    }
}

#[derive(Debug, Clone)]
pub struct SyncTask {
    pub shared_task_id: Uuid,
    pub project_id: Uuid,
    pub title: String,
    pub description: Option<String>,
    pub status: TaskStatus,
    pub activity_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize, Deserialize, TS)]
pub struct UpdateTask {
    pub title: Option<String>,
    pub description: Option<String>,
    pub status: Option<TaskStatus>,
    pub parent_task_id: Option<Uuid>,
    pub image_ids: Option<Vec<Uuid>>,
}

impl Task {
    pub fn to_prompt(&self) -> String {
        if let Some(description) = self.description.as_ref().filter(|d| !d.trim().is_empty()) {
            format!("{}\n\n{}", &self.title, description)
        } else {
            self.title.clone()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::SqlitePool;
    use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode};
    use std::str::FromStr;
    use tempfile::TempDir;

    /// Create a test SQLite pool with migrations applied.
    pub async fn setup_test_pool() -> (SqlitePool, TempDir) {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let db_path = temp_dir.path().join("test.db");

        let options =
            SqliteConnectOptions::from_str(&format!("sqlite://{}", db_path.to_string_lossy()))
                .expect("Invalid database URL")
                .create_if_missing(true)
                .journal_mode(SqliteJournalMode::Wal);

        let pool = SqlitePool::connect_with(options)
            .await
            .expect("Failed to create pool");

        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .expect("Failed to run migrations");

        (pool, temp_dir)
    }

    #[tokio::test]
    async fn test_task_to_prompt_with_description() {
        let task = Task {
            id: Uuid::new_v4(),
            project_id: Uuid::new_v4(),
            title: "Test Title".to_string(),
            description: Some("Test Description".to_string()),
            status: TaskStatus::Todo,
            parent_task_id: None,
            shared_task_id: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            remote_assignee_user_id: None,
            remote_assignee_name: None,
            remote_assignee_username: None,
            remote_version: 0,
            remote_last_synced_at: None,
            remote_stream_node_id: None,
            remote_stream_url: None,
            archived_at: None,
            activity_at: None,
        };

        let prompt = task.to_prompt();
        assert_eq!(prompt, "Test Title\n\nTest Description");
    }

    #[tokio::test]
    async fn test_task_to_prompt_without_description() {
        let task = Task {
            id: Uuid::new_v4(),
            project_id: Uuid::new_v4(),
            title: "Test Title".to_string(),
            description: None,
            status: TaskStatus::Todo,
            parent_task_id: None,
            shared_task_id: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            remote_assignee_user_id: None,
            remote_assignee_name: None,
            remote_assignee_username: None,
            remote_version: 0,
            remote_last_synced_at: None,
            remote_stream_node_id: None,
            remote_stream_url: None,
            archived_at: None,
            activity_at: None,
        };

        let prompt = task.to_prompt();
        assert_eq!(prompt, "Test Title");
    }

    #[tokio::test]
    async fn test_task_to_prompt_with_empty_description() {
        let task = Task {
            id: Uuid::new_v4(),
            project_id: Uuid::new_v4(),
            title: "Test Title".to_string(),
            description: Some("   ".to_string()),
            status: TaskStatus::Todo,
            parent_task_id: None,
            shared_task_id: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            remote_assignee_user_id: None,
            remote_assignee_name: None,
            remote_assignee_username: None,
            remote_version: 0,
            remote_last_synced_at: None,
            remote_stream_node_id: None,
            remote_stream_url: None,
            archived_at: None,
            activity_at: None,
        };

        let prompt = task.to_prompt();
        assert_eq!(prompt, "Test Title");
    }

    #[tokio::test]
    async fn test_create_task_from_title_description() {
        let project_id = Uuid::new_v4();
        let create_task = CreateTask::from_title_description(
            project_id,
            "My Task".to_string(),
            Some("Description".to_string()),
        );

        assert_eq!(create_task.project_id, project_id);
        assert_eq!(create_task.title, "My Task");
        assert_eq!(create_task.description, Some("Description".to_string()));
        assert_eq!(create_task.status, Some(TaskStatus::Todo));
        assert!(create_task.parent_task_id.is_none());
        assert!(create_task.image_ids.is_none());
        assert!(create_task.shared_task_id.is_none());
    }

    #[tokio::test]
    async fn test_create_task_from_shared_task() {
        let project_id = Uuid::new_v4();
        let shared_task_id = Uuid::new_v4();
        let create_task = CreateTask::from_shared_task(
            project_id,
            "Shared Task".to_string(),
            Some("Desc".to_string()),
            TaskStatus::InProgress,
            shared_task_id,
        );

        assert_eq!(create_task.project_id, project_id);
        assert_eq!(create_task.title, "Shared Task");
        assert_eq!(create_task.description, Some("Desc".to_string()));
        assert_eq!(create_task.status, Some(TaskStatus::InProgress));
        assert!(create_task.parent_task_id.is_none());
        assert!(create_task.image_ids.is_none());
        assert_eq!(create_task.shared_task_id, Some(shared_task_id));
    }
}
