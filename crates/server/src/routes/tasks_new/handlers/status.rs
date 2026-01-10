//! Status management handlers: archive, unarchive, assign, get_task_children.

use std::path::PathBuf;

use axum::{
    Extension, Json,
    extract::State,
    http::StatusCode,
    response::Json as ResponseJson,
};
use chrono::Utc;
use db::models::{
    task::Task,
    task_attempt::TaskAttempt,
};
use deployment::Deployment;
use remote::routes::tasks::{AssignSharedTaskRequest, UpdateSharedTaskRequest};
use services::services::{
    container::ContainerService,
    share::status as task_status,
    worktree_manager::{WorktreeCleanup, WorktreeManager},
};
use sqlx::Error as SqlxError;
use utils::response::ApiResponse;

use crate::{DeploymentImpl, error::ApiError};
use crate::routes::tasks_new::types::{
    format_user_display_name, ArchiveTaskRequest, ArchiveTaskResponse,
};

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

    // Tasks synced from Hive are archived by proxying to the Hive API
    if task.shared_task_id.is_some() {
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
