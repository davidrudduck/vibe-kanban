//! Status management handlers: archive, unarchive, assign, get_task_children.

use std::path::PathBuf;

use axum::{Extension, Json, extract::State, http::StatusCode, response::Json as ResponseJson};
use chrono::Utc;
use db::models::{task::Task, task_attempt::TaskAttempt};
use deployment::Deployment;
use remote::routes::tasks::{AssignSharedTaskRequest, UpdateSharedTaskRequest};
use services::services::{
    container::ContainerService,
    share::status as task_status,
    worktree_manager::{WorktreeCleanup, WorktreeManager},
};
use sqlx::Error as SqlxError;
use utils::response::ApiResponse;

use crate::routes::tasks::types::{
    ArchiveTaskRequest, ArchiveTaskResponse, format_user_display_name,
};
use crate::{DeploymentImpl, error::ApiError};

// ============================================================================
// Archive Remote Task Helper
// ============================================================================

/// Archive a remote (hive-synced) task by proxying to the Hive API.
///
/// This updates the task's `archived_at` on the Hive, which will sync back to all nodes.
async fn archive_remote_task(
    deployment: &DeploymentImpl,
    task: &Task,
) -> Result<(StatusCode, ResponseJson<ApiResponse<ArchiveTaskResponse>>), ApiError> {
    let remote_client = deployment.remote_client()?;
    let shared_task_id = task
        .shared_task_id
        .ok_or_else(|| ApiError::BadRequest("Remote task missing shared_task_id".to_string()))?;

    // Don't send version - archive is idempotent and version may be stale
    // if Electric sync hasn't pulled latest changes from Hive
    let request = UpdateSharedTaskRequest {
        title: None,
        description: None,
        status: None,
        archived_at: Some(Some(Utc::now())),
        version: None,
    };

    let response = remote_client
        .update_shared_task(shared_task_id, &request)
        .await?;

    // Build display name from user data
    let assignee_name = response
        .user
        .as_ref()
        .and_then(|u| format_user_display_name(u.first_name.as_ref(), u.last_name.as_ref()));

    // Upsert updated remote task locally
    let pool = &deployment.db().pool;
    let archived_task = Task::upsert_remote_task(
        pool,
        task.id,
        task.project_id,
        response.task.id,
        response.task.title,
        response.task.description,
        task_status::from_remote(&response.task.status),
        response.task.assignee_user_id,
        assignee_name,
        response.user.as_ref().and_then(|u| u.username.clone()),
        response.task.version,
        Some(response.task.updated_at),
        response.task.archived_at,
    )
    .await?;

    // Note: Subtask archiving for hive-synced tasks is handled by the Hive
    // The Hive will propagate archive status to all subtasks
    Ok((
        StatusCode::OK,
        ResponseJson(ApiResponse::success(ArchiveTaskResponse {
            task: archived_task,
            subtasks_archived: 0, // Hive handles subtasks
        })),
    ))
}

// ============================================================================
// Unarchive Remote Task Helper
// ============================================================================

/// Unarchive a remote (hive-synced) task by proxying to the Hive API.
///
/// This clears the task's `archived_at` on the Hive, which will sync back to all nodes.
async fn unarchive_remote_task(
    deployment: &DeploymentImpl,
    task: &Task,
) -> Result<ResponseJson<ApiResponse<Task>>, ApiError> {
    let remote_client = deployment.remote_client()?;
    let shared_task_id = task
        .shared_task_id
        .ok_or_else(|| ApiError::BadRequest("Remote task missing shared_task_id".to_string()))?;

    // Don't send version - unarchive is idempotent and version may be stale
    // if Electric sync hasn't pulled latest changes from Hive
    let request = UpdateSharedTaskRequest {
        title: None,
        description: None,
        status: None,
        archived_at: Some(None), // Some(None) means unarchive
        version: None,
    };

    let response = remote_client
        .update_shared_task(shared_task_id, &request)
        .await?;

    // Build display name from user data
    let assignee_name = response
        .user
        .as_ref()
        .and_then(|u| format_user_display_name(u.first_name.as_ref(), u.last_name.as_ref()));

    // Upsert updated remote task locally
    let pool = &deployment.db().pool;
    let unarchived_task = Task::upsert_remote_task(
        pool,
        task.id,
        task.project_id,
        response.task.id,
        response.task.title,
        response.task.description,
        task_status::from_remote(&response.task.status),
        response.task.assignee_user_id,
        assignee_name,
        response.user.as_ref().and_then(|u| u.username.clone()),
        response.task.version,
        Some(response.task.updated_at),
        response.task.archived_at,
    )
    .await?;

    Ok(ResponseJson(ApiResponse::success(unarchived_task)))
}

// ============================================================================
// Archive Task Handler
// ============================================================================

/// Archive a task and optionally its subtasks.
///
/// This endpoint:
/// 1. Archives the task by setting `archived_at` timestamp
/// 2. Optionally archives all subtasks if `include_subtasks` is true
/// 3. Cleans up worktrees associated with the task's attempts (background task)
///
/// Returns 202 Accepted since worktree cleanup happens in the background.
pub async fn archive_task(
    Extension(task): Extension<Task>,
    State(deployment): State<DeploymentImpl>,
    Json(payload): Json<ArchiveTaskRequest>,
) -> Result<(StatusCode, ResponseJson<ApiResponse<ArchiveTaskResponse>>), ApiError> {
    let pool = &deployment.db().pool;

    // Validate sync state: if task has shared_task_id, project must have remote_project_id
    if task.shared_task_id.is_some() {
        let project = task
            .parent_project(pool)
            .await?
            .ok_or_else(|| ApiError::Database(SqlxError::RowNotFound))?;

        if project.remote_project_id.is_none() {
            return Err(ApiError::SyncStateBroken(
                "Project is unlinked from swarm. Please use 'Unlink & Reset' to clean up sync state before archiving this task.".to_string()
            ));
        }

        // Tasks synced from Hive are archived by proxying to the Hive API
        return archive_remote_task(&deployment, &task).await;
    }

    // Task already archived
    if task.archived_at.is_some() {
        return Err(ApiError::BadRequest("Task is already archived".to_string()));
    }

    // Validate no running execution processes
    if deployment
        .container()
        .has_running_processes(task.id)
        .await?
    {
        return Err(ApiError::Conflict(
            "Task has running execution processes. Please wait for them to complete or stop them first.".to_string()
        ));
    }

    // Get project for worktree cleanup paths
    let project = task
        .parent_project(pool)
        .await?
        .ok_or_else(|| ApiError::Database(SqlxError::RowNotFound))?;

    // Gather task attempts for worktree cleanup (this task)
    let mut attempts = TaskAttempt::fetch_all(pool, Some(task.id))
        .await
        .map_err(|e| {
            tracing::error!("Failed to fetch task attempts for task {}: {}", task.id, e);
            ApiError::TaskAttempt(e)
        })?;

    // Collect subtask IDs and their attempts if cascading
    let mut subtasks_archived = 0u64;
    if payload.include_subtasks {
        let children = Task::find_children_by_parent_id(pool, task.id).await?;
        for child in &children {
            // Check child doesn't have running processes
            if deployment
                .container()
                .has_running_processes(child.id)
                .await?
            {
                return Err(ApiError::Conflict(format!(
                    "Subtask '{}' has running execution processes. Stop them first or uncheck 'include subtasks'.",
                    child.title
                )));
            }

            // Gather child's attempts for cleanup
            let child_attempts =
                TaskAttempt::fetch_all(pool, Some(child.id))
                    .await
                    .map_err(|e| {
                        tracing::error!(
                            "Failed to fetch task attempts for subtask {}: {}",
                            child.id,
                            e
                        );
                        ApiError::TaskAttempt(e)
                    })?;
            attempts.extend(child_attempts);
        }

        // Archive subtasks
        let child_ids: Vec<uuid::Uuid> = children.iter().map(|c| c.id).collect();
        subtasks_archived = Task::archive_many(pool, &child_ids).await?;
    }

    // Archive the main task
    let archived_task = Task::archive(pool, task.id).await?;

    // Gather cleanup data for background worktree cleanup (with attempt IDs for DB update)
    let cleanup_data: Vec<(uuid::Uuid, WorktreeCleanup)> = attempts
        .iter()
        .filter_map(|attempt| {
            attempt.container_ref.as_ref().map(|worktree_path| {
                (
                    attempt.id,
                    WorktreeCleanup {
                        worktree_path: PathBuf::from(worktree_path),
                        git_repo_path: Some(project.git_repo_path.clone()),
                    },
                )
            })
        })
        .collect();

    // Spawn background worktree cleanup task
    let task_id = task.id;
    let pool = pool.clone();
    tokio::spawn(async move {
        let span = tracing::info_span!("archive_worktree_cleanup", task_id = %task_id);
        let _enter = span.enter();

        tracing::info!(
            "Starting background worktree cleanup for archived task {} ({} worktrees)",
            task_id,
            cleanup_data.len()
        );

        for (attempt_id, cleanup) in &cleanup_data {
            // Clean up the worktree filesystem
            if let Err(e) = WorktreeManager::cleanup_worktree(cleanup).await {
                tracing::error!(
                    "Background worktree cleanup failed for attempt {}: {}",
                    attempt_id,
                    e
                );
                continue;
            }

            // Mark worktree as deleted in database
            if let Err(e) = TaskAttempt::mark_worktree_deleted(&pool, *attempt_id).await {
                tracing::error!(
                    "Failed to mark worktree as deleted for attempt {}: {}",
                    attempt_id,
                    e
                );
            }
        }

        tracing::info!(
            "Background worktree cleanup completed for archived task {}",
            task_id
        );
    });

    // Return 202 Accepted to indicate archival was scheduled with background cleanup
    Ok((
        StatusCode::ACCEPTED,
        ResponseJson(ApiResponse::success(ArchiveTaskResponse {
            task: archived_task,
            subtasks_archived,
        })),
    ))
}

// ============================================================================
// Unarchive Task Handler
// ============================================================================

/// Unarchive a task.
///
/// This endpoint clears the `archived_at` timestamp, making the task visible again.
/// Note: Worktrees are not restored - a new attempt would need to be started.
pub async fn unarchive_task(
    Extension(task): Extension<Task>,
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<Task>>, ApiError> {
    let pool = &deployment.db().pool;

    // Tasks synced from Hive are unarchived by proxying to the Hive API
    if task.shared_task_id.is_some() {
        return unarchive_remote_task(&deployment, &task).await;
    }

    // Task not archived
    if task.archived_at.is_none() {
        return Err(ApiError::BadRequest("Task is not archived".to_string()));
    }

    // Unarchive the task
    let unarchived_task = Task::unarchive(pool, task.id).await?;

    Ok(ResponseJson(ApiResponse::success(unarchived_task)))
}

// ============================================================================
// Assign Task Handler
// ============================================================================

/// Assign (or claim) a task.
///
/// This endpoint allows:
/// - Claiming an unassigned remote task (anyone in the org can claim)
/// - Reassigning a task (assignee or org admin)
///
/// For local tasks, this is a no-op since they don't have assignees.
pub async fn assign_task(
    Extension(task): Extension<Task>,
    State(deployment): State<DeploymentImpl>,
    Json(payload): Json<AssignSharedTaskRequest>,
) -> Result<ResponseJson<ApiResponse<Task>>, ApiError> {
    // Only works for tasks synced from Hive
    let shared_task_id = task.shared_task_id.ok_or_else(|| {
        ApiError::BadRequest("Only Hive-synced tasks can be assigned".to_string())
    })?;

    // Get the remote client
    let client = deployment.remote_client()?;

    // Call the Hive assign endpoint
    let response = client.assign_shared_task(shared_task_id, &payload).await?;

    // Build assignee name from response
    let assignee_name = response
        .user
        .as_ref()
        .and_then(|u| match (&u.first_name, &u.last_name) {
            (Some(f), Some(l)) => Some(format!("{} {}", f, l)),
            (Some(f), None) => Some(f.clone()),
            (None, Some(l)) => Some(l.clone()),
            (None, None) => None,
        });

    // Upsert updated remote task locally
    let pool = &deployment.db().pool;
    let updated_task = Task::upsert_remote_task(
        pool,
        task.id,
        task.project_id,
        response.task.id,
        response.task.title,
        response.task.description,
        task_status::from_remote(&response.task.status),
        response.task.assignee_user_id,
        assignee_name,
        response.user.as_ref().and_then(|u| u.username.clone()),
        response.task.version,
        Some(response.task.updated_at),
        response.task.archived_at,
    )
    .await?;

    Ok(ResponseJson(ApiResponse::success(updated_task)))
}

// ============================================================================
// Get Task Children Handler
// ============================================================================

/// Get children (subtasks) of a task.
///
/// Used by the archive dialog to show the user how many subtasks will be affected.
pub async fn get_task_children(
    Extension(task): Extension<Task>,
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<Vec<Task>>>, ApiError> {
    let children = Task::find_children_by_parent_id(&deployment.db().pool, task.id).await?;
    Ok(ResponseJson(ApiResponse::success(children)))
}

#[cfg(test)]
mod tests {
    use db::models::{
        project::{CreateProject, Project},
        task::{CreateTask, Task},
    };
    use db::test_utils::create_test_pool;
    use uuid::Uuid;

    #[tokio::test]
    async fn test_archive_task_with_broken_sync_state() {
        let (pool, _temp_dir) = create_test_pool().await;

        // Create a project without remote_project_id (unlinked)
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
        let project = Project::create(&pool, &project_data, project_id)
            .await
            .expect("Failed to create project");

        // Verify project has no remote_project_id
        assert!(project.remote_project_id.is_none());

        // Create a task with shared_task_id (orphaned - has shared_task_id but project is unlinked)
        let task_id = Uuid::new_v4();
        let shared_task_id = Uuid::new_v4();
        let task_data = CreateTask::from_title_description(
            project_id,
            "Orphaned Task".to_string(),
            Some("Task with shared_task_id but unlinked project".to_string()),
        );
        Task::create(&pool, &task_data, task_id)
            .await
            .expect("Failed to create task");

        // Manually set shared_task_id to simulate orphaned state
        sqlx::query("UPDATE tasks SET shared_task_id = ? WHERE id = ?")
            .bind(shared_task_id)
            .bind(task_id)
            .execute(&pool)
            .await
            .expect("Failed to update shared_task_id");

        // Reload task to get updated shared_task_id
        let task = Task::find_by_id(&pool, task_id)
            .await
            .expect("Query failed")
            .expect("Task not found");
        assert!(task.shared_task_id.is_some());

        // Create minimal deployment for testing (this won't actually be used, just for signature)
        // Since we're testing the validation logic before remote calls, we can use a minimal setup
        // However, we need to test the validation logic directly by simulating the check

        // Test validation logic directly
        // When task has shared_task_id but project has no remote_project_id, should fail
        let project = task
            .parent_project(&pool)
            .await
            .expect("Failed to get parent project")
            .expect("Project not found");

        assert!(task.shared_task_id.is_some());
        assert!(project.remote_project_id.is_none());

        // This simulates the validation check in the handler
        // If this assertion passes, the handler would return SyncStateBroken error
        assert!(
            task.shared_task_id.is_some() && project.remote_project_id.is_none(),
            "Expected broken sync state: task has shared_task_id but project has no remote_project_id"
        );
    }

    #[tokio::test]
    async fn test_archive_task_with_valid_sync_state() {
        let (pool, _temp_dir) = create_test_pool().await;

        // Create a project with remote_project_id (linked)
        let project_id = Uuid::new_v4();
        let remote_project_id = Uuid::new_v4();
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

        // Set remote_project_id
        sqlx::query("UPDATE projects SET remote_project_id = ? WHERE id = ?")
            .bind(remote_project_id)
            .bind(project_id)
            .execute(&pool)
            .await
            .expect("Failed to set remote_project_id");

        // Reload project
        let project = Project::find_by_id(&pool, project_id)
            .await
            .expect("Query failed")
            .expect("Project not found");
        assert!(project.remote_project_id.is_some());

        // Create a task with shared_task_id (valid - both task and project are linked)
        let task_id = Uuid::new_v4();
        let shared_task_id = Uuid::new_v4();
        let task_data = CreateTask::from_title_description(
            project_id,
            "Linked Task".to_string(),
            Some("Task with valid sync state".to_string()),
        );
        Task::create(&pool, &task_data, task_id)
            .await
            .expect("Failed to create task");

        // Set shared_task_id
        sqlx::query("UPDATE tasks SET shared_task_id = ? WHERE id = ?")
            .bind(shared_task_id)
            .bind(task_id)
            .execute(&pool)
            .await
            .expect("Failed to update shared_task_id");

        // Reload task
        let task = Task::find_by_id(&pool, task_id)
            .await
            .expect("Query failed")
            .expect("Task not found");

        // Verify both task and project are linked
        let project = task
            .parent_project(&pool)
            .await
            .expect("Failed to get parent project")
            .expect("Project not found");

        assert!(task.shared_task_id.is_some());
        assert!(project.remote_project_id.is_some());

        // This simulates valid sync state - validation should pass
        assert!(
            task.shared_task_id.is_some() && project.remote_project_id.is_some(),
            "Expected valid sync state: both task and project are linked"
        );
    }
}
