//! Request/response types for task routes.

use db::models::task::{CreateTask, Task};
use executors::profile::ExecutorProfileId;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

/// Format display name from first/last name fields
pub fn format_user_display_name(
    first_name: Option<&String>,
    last_name: Option<&String>,
) -> Option<String> {
    match (first_name, last_name) {
        (Some(f), Some(l)) => Some(format!("{} {}", f, l)),
        (Some(f), None) => Some(f.clone()),
        (None, Some(l)) => Some(l.clone()),
        (None, None) => None,
    }
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TaskQuery {
    pub project_id: Uuid,
    #[serde(default)]
    pub include_archived: bool,
}

#[derive(Debug, Deserialize, TS)]
pub struct CreateAndStartTaskRequest {
    pub task: CreateTask,
    pub executor_profile_id: ExecutorProfileId,
    pub base_branch: String,
    /// When true, reuse the parent task's latest attempt worktree.
    /// Only valid when the task has a parent_task_id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub use_parent_worktree: Option<bool>,
}

/// Request body for archiving a task
#[derive(Debug, Deserialize, TS)]
pub struct ArchiveTaskRequest {
    /// Whether to also archive subtasks (children). Defaults to true.
    #[serde(default = "default_true")]
    pub include_subtasks: bool,
}

/// Response from archive/unarchive operations
#[derive(Debug, Serialize, Deserialize, TS)]
pub struct ArchiveTaskResponse {
    /// The archived/unarchived task
    pub task: Task,
    /// Number of subtasks also archived (only for archive operation)
    pub subtasks_archived: u64,
}
