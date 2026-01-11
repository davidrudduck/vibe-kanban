//! Remote/Hive task helpers: create, update, delete, and resync operations.
//!
//! These are pub(crate) functions called by core.rs handlers when interacting with
//! remote tasks synced to/from the Hive.

use axum::{http::StatusCode, response::Json as ResponseJson};
use db::models::{project::Project, task::Task};
use deployment::Deployment;
use remote::routes::tasks::{CreateSharedTaskRequest, DeleteSharedTaskRequest, UpdateSharedTaskRequest};
use services::services::share::status as task_status;
use utils::response::ApiResponse;
use uuid::Uuid;

use crate::{DeploymentImpl, error::ApiError};
use crate::routes::tasks::types::format_user_display_name;

// ============================================================================
// Create Remote Task
// ============================================================================

/// Create a task on a remote project by proxying to the Hive.
///
/// Called by `create_task` when the project is marked as remote.
pub(crate) async fn create_remote_task(
    deployment: &DeploymentImpl,
    project: &Project,
    payload: &db::models::task::CreateTask,
) -> Result<ResponseJson<ApiResponse<Task>>, ApiError> {
    // Remote tasks don't support images
    if payload
        .image_ids
        .as_ref()
        .is_some_and(|ids| !ids.is_empty())
    {
        return Err(ApiError::BadRequest(
            "Image attachments are not supported for remote project tasks".to_string(),
        ));
    }

    let remote_client = deployment.remote_client()?;
    let remote_project_id = project.remote_project_id.ok_or_else(|| {
        ApiError::BadRequest("Remote project missing remote_project_id".to_string())
    })?;

    let request = CreateSharedTaskRequest {
        project_id: remote_project_id,
        title: payload.title.clone(),
        description: payload.description.clone(),
        status: None, // Default to Todo on the Hive
        assignee_user_id: None,
        start_attempt: false, // Do not auto-dispatch for remote projects created from local node
        source_task_id: None, // Not a re-sync operation
        source_node_id: None,
    };

    let response = remote_client.create_shared_task(&request).await?;

    // Build display name from user data
    let assignee_name = response
        .user
        .as_ref()
        .and_then(|u| format_user_display_name(u.first_name.as_ref(), u.last_name.as_ref()));

    // Upsert as remote task locally
    let pool = &deployment.db().pool;
    let task = Task::upsert_remote_task(
        pool,
        Uuid::new_v4(),
        project.id,
        response.task.id,
        response.task.title,
        response.task.description,
        task_status::from_remote(&response.task.status),
        response.task.assignee_user_id,
        assignee_name,
        response.user.as_ref().and_then(|u| u.username.clone()),
        response.task.version,
        Some(response.task.updated_at), // Use updated_at as activity_at for new tasks
        response.task.archived_at,
    )
    .await?;

    tracing::info!(
        task_id = %task.id,
        shared_task_id = ?task.shared_task_id,
        project_id = %project.id,
        "Created remote task via Hive"
    );

    Ok(ResponseJson(ApiResponse::success(task)))
}

// ============================================================================
// Update Remote Task
// ============================================================================

/// Update a remote task by proxying to the Hive.
///
/// Called by `update_task` when the task has a `shared_task_id`.
pub(crate) async fn update_remote_task(
    deployment: &DeploymentImpl,
    existing_task: &Task,
    payload: &db::models::task::UpdateTask,
) -> Result<ResponseJson<ApiResponse<Task>>, ApiError> {
    // Remote tasks don't support images or parent_task_id
    if payload
        .image_ids
        .as_ref()
        .is_some_and(|ids| !ids.is_empty())
    {
        return Err(ApiError::BadRequest(
            "Image attachments are not supported for remote project tasks".to_string(),
        ));
    }
    if payload.parent_task_id.is_some() {
        return Err(ApiError::BadRequest(
            "Parent task is not supported for remote project tasks".to_string(),
        ));
    }

    let remote_client = deployment.remote_client()?;
    let shared_task_id = existing_task
        .shared_task_id
        .ok_or_else(|| ApiError::BadRequest("Remote task missing shared_task_id".to_string()))?;

    let request = UpdateSharedTaskRequest {
        title: payload.title.clone(),
        description: payload.description.clone(),
        status: payload.status.as_ref().map(task_status::to_remote),
        archived_at: None, // Don't modify archived_at when updating a remote task
        version: Some(existing_task.remote_version),
    };

    let pool = &deployment.db().pool;

    // Try to update on Hive
    match remote_client
        .update_shared_task(shared_task_id, &request)
        .await
    {
        Ok(response) => {
            // Build display name from user data
            let assignee_name = response.user.as_ref().and_then(|u| {
                format_user_display_name(u.first_name.as_ref(), u.last_name.as_ref())
            });

            // Upsert updated remote task locally
            let task = Task::upsert_remote_task(
                pool,
                existing_task.id,
                existing_task.project_id,
                response.task.id,
                response.task.title,
                response.task.description,
                task_status::from_remote(&response.task.status),
                response.task.assignee_user_id,
                assignee_name,
                response.user.as_ref().and_then(|u| u.username.clone()),
                response.task.version,
                Some(response.task.updated_at), // Use updated_at as activity_at for task updates
                response.task.archived_at,
            )
            .await?;

            tracing::info!(
                task_id = %task.id,
                shared_task_id = ?task.shared_task_id,
                "Updated remote task via Hive"
            );

            Ok(ResponseJson(ApiResponse::success(task)))
        }
        Err(e) if e.is_not_found() => {
            // Task doesn't exist on Hive - re-sync the task
            tracing::warn!(
                task_id = %existing_task.id,
                shared_task_id = %shared_task_id,
                "Shared task not found on Hive, re-syncing"
            );

            let task = resync_task_to_hive(
                deployment,
                existing_task,
                payload.title.clone(),
                payload.description.clone(),
                payload.status.clone(),
            )
            .await?;

            Ok(ResponseJson(ApiResponse::success(task)))
        }
        Err(e) => Err(e.into()),
    }
}

// ============================================================================
// Delete Remote Task
// ============================================================================

/// Delete a remote task by proxying to the Hive.
///
/// Called by `delete_task` when the task has a `shared_task_id`.
pub(crate) async fn delete_remote_task(
    deployment: &DeploymentImpl,
    task: &Task,
) -> Result<(StatusCode, ResponseJson<ApiResponse<()>>), ApiError> {
    // If task has shared_task_id, delete from Hive
    // The WebSocket handler will receive the deletion event and clean up locally
    if let Some(shared_task_id) = task.shared_task_id {
        let remote_client = deployment.remote_client()?;
        let request = DeleteSharedTaskRequest { version: None };
        remote_client
            .delete_shared_task(shared_task_id, &request)
            .await?;

        tracing::info!(
            task_id = %task.id,
            shared_task_id = %shared_task_id,
            "Deleted remote task via Hive; local cache will be cleaned by WebSocket sync"
        );
        // NOTE: Do NOT delete locally here - WebSocket handler will process
        // the "task.deleted" event and clean up the local cache in a transaction
    } else {
        // Task is marked remote but has no shared_task_id (sync never completed)
        // Delete locally since there's no Hive event to sync from
        let pool = &deployment.db().pool;
        tracing::warn!(
            task_id = %task.id,
            "Deleting remote task that was never synced to Hive (no shared_task_id)"
        );
        Task::delete(pool, task.id).await?;
    }

    // Return 202 Accepted to match local delete behavior
    Ok((StatusCode::ACCEPTED, ResponseJson(ApiResponse::success(()))))
}

// ============================================================================
// Resync Task to Hive
// ============================================================================

/// Re-sync a task to the Hive when its shared_task_id is stale.
///
/// This is called when an update or label operation returns 404 from the Hive,
/// indicating that the shared_task_id no longer exists. The task is re-created
/// on the Hive with source tracking to prevent duplicates.
///
/// If the node is not connected to the Hive (no node_id available), this function
/// will clear the stale shared_task_id and update the task locally instead.
pub(crate) async fn resync_task_to_hive(
    deployment: &DeploymentImpl,
    existing_task: &Task,
    title: Option<String>,
    description: Option<String>,
    status: Option<db::models::task::TaskStatus>,
) -> Result<Task, ApiError> {
    let pool = &deployment.db().pool;

    // Check if node is connected to Hive
    let node_id = deployment.node_proxy_client().local_node_id();

    if node_id.is_none() {
        // Node not connected to Hive - clear stale shared_task_id and update locally
        tracing::warn!(
            task_id = %existing_task.id,
            old_shared_task_id = ?existing_task.shared_task_id,
            "Cannot resync task: node not connected to Hive. Clearing stale shared_task_id."
        );

        // Clear the shared_task_id and update the task locally
        let task = Task::clear_shared_task_id(pool, existing_task.id).await?;

        // Now update the task locally with the requested changes
        let task = Task::update(
            pool,
            task.id,
            task.project_id,
            title.unwrap_or(task.title),
            description.or(task.description),
            status.unwrap_or(task.status),
            task.parent_task_id,
        )
        .await?;

        return Ok(task);
    }

    let node_id = node_id.unwrap();
    let remote_client = deployment.remote_client()?;

    // Get the project's remote_project_id
    let project = Project::find_by_id(pool, existing_task.project_id)
        .await?
        .ok_or_else(|| ApiError::NotFound("Project not found".to_string()))?;

    let remote_project_id = project
        .remote_project_id
        .ok_or_else(|| ApiError::BadRequest("Project not linked to Hive".to_string()))?;

    // Create the task on the Hive with source tracking
    let request = CreateSharedTaskRequest {
        project_id: remote_project_id,
        title: title.unwrap_or_else(|| existing_task.title.clone()),
        description: description.or_else(|| existing_task.description.clone()),
        status: status.map(|s| task_status::to_remote(&s)),
        assignee_user_id: None,
        start_attempt: false,
        source_task_id: Some(existing_task.id),
        source_node_id: Some(node_id),
    };

    let response = remote_client.create_shared_task(&request).await?;

    // Build display name from user data
    let assignee_name = response
        .user
        .as_ref()
        .and_then(|u| format_user_display_name(u.first_name.as_ref(), u.last_name.as_ref()));

    // Update local task with the new shared_task_id
    let task = Task::upsert_remote_task(
        pool,
        existing_task.id,
        existing_task.project_id,
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

    tracing::info!(
        task_id = %task.id,
        old_shared_task_id = ?existing_task.shared_task_id,
        new_shared_task_id = ?task.shared_task_id,
        "Re-synced task to Hive with new shared_task_id"
    );

    Ok(task)
}
