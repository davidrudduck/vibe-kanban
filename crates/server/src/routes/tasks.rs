use std::path::PathBuf;

use anyhow;
use axum::{
    Extension, Json, Router,
    extract::{
        Query, State,
        ws::{WebSocket, WebSocketUpgrade},
    },
    http::StatusCode,
    middleware::from_fn_with_state,
    response::{IntoResponse, Json as ResponseJson},
    routing::{delete, get, post, put},
};
use db::models::{
    image::TaskImage,
    label::{Label, SetTaskLabels},
    project::Project,
    task::{CreateTask, Task, TaskWithAttemptStatus, UpdateTask},
    task_attempt::{CreateTaskAttempt, TaskAttempt, TaskAttemptError},
};
use deployment::Deployment;
use executors::profile::ExecutorProfileId;
use futures_util::TryStreamExt;
use remote::routes::{
    projects::ListProjectNodesResponse,
    tasks::{
        CreateSharedTaskRequest, DeleteSharedTaskRequest, TaskStreamConnectionInfoResponse,
        UpdateSharedTaskRequest,
    },
};
use serde::{Deserialize, Serialize};
use services::services::{
    container::ContainerService,
    share::{ShareError, status as task_status},
    worktree_manager::{WorktreeCleanup, WorktreeManager},
};
use sqlx::Error as SqlxError;
use ts_rs::TS;
use utils::response::ApiResponse;
use uuid::Uuid;

use crate::{
    DeploymentImpl,
    error::ApiError,
    middleware::load_task_middleware,
    ws_util::{WsKeepAlive, run_ws_stream},
};

/// Format display name from first/last name fields
fn format_user_display_name(
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

#[derive(Debug, Serialize, Deserialize)]
pub struct TaskQuery {
    pub project_id: Uuid,
    #[serde(default)]
    pub include_archived: bool,
}

pub async fn get_tasks(
    State(deployment): State<DeploymentImpl>,
    Query(query): Query<TaskQuery>,
) -> Result<ResponseJson<ApiResponse<Vec<TaskWithAttemptStatus>>>, ApiError> {
    let tasks = Task::find_by_project_id_with_attempt_status(
        &deployment.db().pool,
        query.project_id,
        query.include_archived,
    )
    .await?;

    Ok(ResponseJson(ApiResponse::success(tasks)))
}

pub async fn stream_tasks_ws(
    ws: WebSocketUpgrade,
    State(deployment): State<DeploymentImpl>,
    Query(query): Query<TaskQuery>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| async move {
        if let Err(e) =
            handle_tasks_ws(socket, deployment, query.project_id, query.include_archived).await
        {
            tracing::warn!("tasks WS closed: {}", e);
        }
    })
}

async fn handle_tasks_ws(
    socket: WebSocket,
    deployment: DeploymentImpl,
    project_id: Uuid,
    include_archived: bool,
) -> anyhow::Result<()> {
    // Get the raw stream and convert LogMsg to WebSocket messages
    let stream = deployment
        .events()
        .stream_tasks_raw(project_id, include_archived)
        .await?
        .map_ok(|msg| msg.to_ws_message_unchecked());

    // Use run_ws_stream for proper keep-alive handling
    run_ws_stream(socket, stream, WsKeepAlive::for_list_streams()).await
}

pub async fn get_task(
    Extension(task): Extension<Task>,
    State(_deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<Task>>, ApiError> {
    Ok(ResponseJson(ApiResponse::success(task)))
}

pub async fn create_task(
    State(deployment): State<DeploymentImpl>,
    Json(payload): Json<CreateTask>,
) -> Result<ResponseJson<ApiResponse<Task>>, ApiError> {
    let pool = &deployment.db().pool;

    // Check if project is remote
    let project = Project::find_by_id(pool, payload.project_id)
        .await?
        .ok_or(ApiError::Database(SqlxError::RowNotFound))?;

    if project.is_remote {
        // Remote project: proxy to Hive and sync locally
        return create_remote_task(&deployment, &project, &payload).await;
    }

    // Local project: existing logic

    // Validate: If creating a subtask, check that the parent doesn't use a shared worktree
    if let Some(parent_task_id) = payload.parent_task_id {
        let parent_task = Task::find_by_id(pool, parent_task_id)
            .await?
            .ok_or(ApiError::Database(SqlxError::RowNotFound))?;

        // Check if parent task uses a shared worktree from its own parent (grandparent)
        if let Some(grandparent_id) = parent_task.parent_task_id {
            let uses_shared =
                TaskAttempt::task_uses_shared_worktree(pool, parent_task_id, grandparent_id)
                    .await
                    .map_err(|e| ApiError::TaskAttempt(TaskAttemptError::Database(e)))?;

            if uses_shared {
                return Err(ApiError::BadRequest(
                    "Cannot create subtask: parent task uses a shared worktree".to_string(),
                ));
            }
        }
    }

    let id = Uuid::new_v4();

    tracing::debug!(
        "Creating task '{}' in project {}",
        payload.title,
        payload.project_id
    );

    let task = Task::create(pool, &payload, id).await?;

    if let Some(image_ids) = &payload.image_ids {
        TaskImage::associate_many_dedup(pool, task.id, image_ids).await?;
    }

    // Auto-share task if project is linked to the Hive
    let mut task = task;
    if project.remote_project_id.is_some()
        && let Ok(publisher) = deployment.share_publisher()
    {
        // Get user_id for sharing - use cached profile if available (optional)
        let user_id = deployment
            .auth_context()
            .cached_profile()
            .await
            .map(|p| p.user_id);

        match publisher.share_task(task.id, user_id).await {
            Ok(shared_task_id) => {
                tracing::info!(
                    task_id = %task.id,
                    shared_task_id = %shared_task_id,
                    "Auto-shared task to Hive"
                );
                // Update local task with shared_task_id for consistency
                if let Some(updated) = Task::find_by_id(pool, task.id).await? {
                    task = updated;
                }
            }
            Err(e) => {
                tracing::warn!(
                    task_id = %task.id,
                    error = ?e,
                    "Failed to auto-share task to Hive"
                );
            }
        }
    }

    Ok(ResponseJson(ApiResponse::success(task)))
}

/// Create a task on a remote project by proxying to the Hive
async fn create_remote_task(
    deployment: &DeploymentImpl,
    project: &Project,
    payload: &CreateTask,
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

#[derive(Debug, Deserialize, TS)]
pub struct CreateAndStartTaskRequest {
    pub task: CreateTask,
    pub executor_profile_id: ExecutorProfileId,
    pub base_branch: String,
}

pub async fn create_task_and_start(
    State(deployment): State<DeploymentImpl>,
    Json(payload): Json<CreateAndStartTaskRequest>,
) -> Result<ResponseJson<ApiResponse<TaskWithAttemptStatus>>, ApiError> {
    let pool = &deployment.db().pool;

    // Check if project is remote - cannot start task attempts on remote projects
    let project = Project::find_by_id(pool, payload.task.project_id)
        .await?
        .ok_or(ApiError::Database(SqlxError::RowNotFound))?;

    if project.is_remote {
        return Err(ApiError::BadRequest(
            "Cannot start task attempts on remote projects. Tasks execute on their origin node."
                .to_string(),
        ));
    }

    let task_id = Uuid::new_v4();
    let task = Task::create(pool, &payload.task, task_id).await?;

    if let Some(image_ids) = &payload.task.image_ids {
        TaskImage::associate_many(pool, task.id, image_ids).await?;
    }

    // Auto-share task if project is linked to the Hive
    if let Some(project) = Project::find_by_id(pool, task.project_id).await?
        && project.remote_project_id.is_some()
        && let Ok(publisher) = deployment.share_publisher()
    {
        // Get user_id for sharing - use cached profile if available (optional)
        let user_id = deployment
            .auth_context()
            .cached_profile()
            .await
            .map(|p| p.user_id);

        match publisher.share_task(task.id, user_id).await {
            Ok(shared_task_id) => {
                tracing::info!(
                    task_id = %task.id,
                    shared_task_id = %shared_task_id,
                    "Auto-shared task to Hive"
                );
            }
            Err(e) => {
                tracing::warn!(
                    task_id = %task.id,
                    error = ?e,
                    "Failed to auto-share task to Hive"
                );
            }
        }
    }

    let attempt_id = Uuid::new_v4();
    let git_branch_name = deployment
        .container()
        .git_branch_from_task_attempt(&attempt_id, &task.title)
        .await;

    let task_attempt = TaskAttempt::create(
        pool,
        &CreateTaskAttempt {
            executor: payload.executor_profile_id.executor,
            base_branch: payload.base_branch,
            branch: git_branch_name,
        },
        attempt_id,
        task.id,
    )
    .await?;
    let is_attempt_running = deployment
        .container()
        .start_attempt(&task_attempt, payload.executor_profile_id.clone(), false)
        .await
        .inspect_err(|err| tracing::error!("Failed to start task attempt: {}", err))
        .is_ok();

    let task = Task::find_by_id(pool, task.id)
        .await?
        .ok_or(ApiError::Database(SqlxError::RowNotFound))?;

    tracing::info!("Started attempt for task {}", task.id);
    Ok(ResponseJson(ApiResponse::success(TaskWithAttemptStatus {
        task,
        has_in_progress_attempt: is_attempt_running,
        has_merged_attempt: false,
        last_attempt_failed: false,
        executor: task_attempt.executor,
    })))
}

pub async fn update_task(
    Extension(existing_task): Extension<Task>,
    State(deployment): State<DeploymentImpl>,

    Json(payload): Json<UpdateTask>,
) -> Result<ResponseJson<ApiResponse<Task>>, ApiError> {
    // Check if this is a remote task
    if existing_task.is_remote {
        return update_remote_task(&deployment, &existing_task, &payload).await;
    }

    // Local task: existing logic
    // Use existing values if not provided in update
    let title = payload.title.unwrap_or(existing_task.title);
    let description = match payload.description {
        Some(s) if s.trim().is_empty() => None, // Empty string = clear description
        Some(s) => Some(s),                     // Non-empty string = update description
        None => existing_task.description,      // Field omitted = keep existing
    };
    let status = payload.status.unwrap_or(existing_task.status);
    let parent_task_id = payload.parent_task_id.or(existing_task.parent_task_id);
    // Handle validation_steps: if provided use it, otherwise keep existing
    let validation_steps = match &payload.validation_steps {
        Some(s) if s.trim().is_empty() => None, // Empty string = clear validation steps
        Some(s) => Some(s.clone()),             // Non-empty string = update
        None => existing_task.validation_steps, // Field omitted = keep existing
    };

    let task = Task::update(
        &deployment.db().pool,
        existing_task.id,
        existing_task.project_id,
        title,
        description,
        status,
        parent_task_id,
        validation_steps,
    )
    .await?;

    if let Some(image_ids) = &payload.image_ids {
        TaskImage::delete_by_task_id(&deployment.db().pool, task.id).await?;
        TaskImage::associate_many_dedup(&deployment.db().pool, task.id, image_ids).await?;
    }

    // If task has been shared, broadcast update (fire-and-forget to avoid blocking)
    if task.shared_task_id.is_some()
        && let Ok(publisher) = deployment.share_publisher()
    {
        let task_clone = task.clone();
        tokio::spawn(async move {
            match tokio::time::timeout(
                std::time::Duration::from_secs(5),
                publisher.update_shared_task(&task_clone),
            )
            .await
            {
                Ok(Ok(())) => {}
                Ok(Err(e)) => tracing::warn!(?e, "failed to sync shared task update"),
                Err(_) => tracing::warn!("shared task sync timed out"),
            }
        });
    }

    Ok(ResponseJson(ApiResponse::success(task)))
}

/// Update a remote task by proxying to the Hive
async fn update_remote_task(
    deployment: &DeploymentImpl,
    existing_task: &Task,
    payload: &UpdateTask,
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
        version: Some(existing_task.remote_version),
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
    )
    .await?;

    tracing::info!(
        task_id = %task.id,
        shared_task_id = ?task.shared_task_id,
        "Updated remote task via Hive"
    );

    Ok(ResponseJson(ApiResponse::success(task)))
}

pub async fn delete_task(
    Extension(task): Extension<Task>,
    State(deployment): State<DeploymentImpl>,
) -> Result<(StatusCode, ResponseJson<ApiResponse<()>>), ApiError> {
    // Check if this is a remote task
    if task.is_remote {
        return delete_remote_task(&deployment, &task).await;
    }

    // Local task: existing logic
    // Validate no running execution processes
    if deployment
        .container()
        .has_running_processes(task.id)
        .await?
    {
        return Err(ApiError::Conflict("Task has running execution processes. Please wait for them to complete or stop them first.".to_string()));
    }

    // Gather task attempts data needed for background cleanup
    let attempts = TaskAttempt::fetch_all(&deployment.db().pool, Some(task.id))
        .await
        .map_err(|e| {
            tracing::error!("Failed to fetch task attempts for task {}: {}", task.id, e);
            ApiError::TaskAttempt(e)
        })?;

    // Gather cleanup data before deletion
    let project = task
        .parent_project(&deployment.db().pool)
        .await?
        .ok_or_else(|| ApiError::Database(SqlxError::RowNotFound))?;

    let cleanup_args: Vec<WorktreeCleanup> = attempts
        .iter()
        .filter_map(|attempt| {
            attempt
                .container_ref
                .as_ref()
                .map(|worktree_path| WorktreeCleanup {
                    worktree_path: PathBuf::from(worktree_path),
                    git_repo_path: Some(project.git_repo_path.clone()),
                })
        })
        .collect();

    // Fire-and-forget remote deletion to avoid blocking local operation
    if let Some(shared_task_id) = task.shared_task_id
        && let Ok(publisher) = deployment.share_publisher()
    {
        tokio::spawn(async move {
            match tokio::time::timeout(
                std::time::Duration::from_secs(5),
                publisher.delete_shared_task(shared_task_id),
            )
            .await
            {
                Ok(Ok(())) => {}
                Ok(Err(e)) => tracing::warn!(?e, "failed to sync shared task deletion"),
                Err(_) => tracing::warn!("shared task deletion sync timed out"),
            }
        });
    }

    // Use a transaction to ensure atomicity: either all operations succeed or all are rolled back
    let mut tx = deployment.db().pool.begin().await?;

    // Nullify parent_task_id for all child tasks before deletion
    // This breaks parent-child relationships to avoid foreign key constraint violations
    let total_children_affected = Task::nullify_children_by_parent_id(&mut *tx, task.id).await?;

    // Delete task from database (FK CASCADE will handle task_attempts)
    let rows_affected = Task::delete(&mut *tx, task.id).await?;

    if rows_affected == 0 {
        return Err(ApiError::Database(SqlxError::RowNotFound));
    }

    // Commit the transaction - if this fails, all changes are rolled back
    tx.commit().await?;

    if total_children_affected > 0 {
        tracing::info!(
            "Nullified {} child task references before deleting task {}",
            total_children_affected,
            task.id
        );
    }

    // Spawn background worktree cleanup task
    let task_id = task.id;
    tokio::spawn(async move {
        let span = tracing::info_span!("background_worktree_cleanup", task_id = %task_id);
        let _enter = span.enter();

        tracing::info!(
            "Starting background cleanup for task {} ({} worktrees)",
            task_id,
            cleanup_args.len()
        );

        if let Err(e) = WorktreeManager::batch_cleanup_worktrees(&cleanup_args).await {
            tracing::error!(
                "Background worktree cleanup failed for task {}: {}",
                task_id,
                e
            );
        } else {
            tracing::info!("Background cleanup completed for task {}", task_id);
        }
    });

    // Return 202 Accepted to indicate deletion was scheduled
    Ok((StatusCode::ACCEPTED, ResponseJson(ApiResponse::success(()))))
}

/// Delete a remote task by proxying to the Hive
async fn delete_remote_task(
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

#[derive(Debug, Serialize, Deserialize, TS)]
pub struct ShareTaskResponse {
    pub shared_task_id: Uuid,
}

pub async fn share_task(
    Extension(task): Extension<Task>,
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<ShareTaskResponse>>, ApiError> {
    let Ok(publisher) = deployment.share_publisher() else {
        return Err(ShareError::MissingConfig("share publisher unavailable").into());
    };
    let profile = deployment
        .auth_context()
        .cached_profile()
        .await
        .ok_or(ShareError::MissingAuth)?;
    let shared_task_id = publisher.share_task(task.id, Some(profile.user_id)).await?;

    Ok(ResponseJson(ApiResponse::success(ShareTaskResponse {
        shared_task_id,
    })))
}

/// Get list of nodes where this task's project exists (for remote attempt start).
///
/// Returns nodes that have the task's project linked, allowing the frontend
/// to show a node selector when starting a task attempt remotely.
pub async fn get_available_nodes(
    Extension(task): Extension<Task>,
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<ListProjectNodesResponse>>, ApiError> {
    let pool = &deployment.db().pool;

    // Get the task's project to find the remote_project_id
    let project = Project::find_by_id(pool, task.project_id)
        .await?
        .ok_or_else(|| ApiError::BadRequest("Project not found".to_string()))?;

    // If the project is not linked to the hive, return empty list
    let Some(remote_project_id) = project.remote_project_id else {
        return Ok(ResponseJson(ApiResponse::success(
            ListProjectNodesResponse { nodes: vec![] },
        )));
    };

    // Query the hive for nodes that have this project linked
    let client = deployment.remote_client()?;
    let response = client.list_project_nodes(remote_project_id).await?;

    Ok(ResponseJson(ApiResponse::success(response)))
}

/// Get stream connection info for a remote task.
///
/// Returns connection URLs and token to stream diff/log data directly from
/// the remote node where the task attempt is running.
pub async fn get_stream_connection_info(
    Extension(task): Extension<Task>,
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<TaskStreamConnectionInfoResponse>>, ApiError> {
    // This endpoint is only valid for remote tasks
    let shared_task_id = task.shared_task_id.ok_or_else(|| {
        ApiError::BadRequest("Task is not a remote task or has no shared_task_id".to_string())
    })?;

    // Call the hive to get connection info
    let client = deployment.remote_client()?;
    let response = client
        .get_task_stream_connection_info(shared_task_id)
        .await?;

    Ok(ResponseJson(ApiResponse::success(response)))
}

/// Request body for archiving a task
#[derive(Debug, Deserialize, TS)]
pub struct ArchiveTaskRequest {
    /// Whether to also archive subtasks (children). Defaults to true.
    #[serde(default = "default_true")]
    pub include_subtasks: bool,
}

fn default_true() -> bool {
    true
}

/// Response from archive/unarchive operations
#[derive(Debug, Serialize, Deserialize, TS)]
pub struct ArchiveTaskResponse {
    /// The archived/unarchived task
    pub task: Task,
    /// Number of subtasks also archived (only for archive operation)
    pub subtasks_archived: u64,
}

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

    // Remote tasks cannot be archived locally
    if task.is_remote {
        return Err(ApiError::BadRequest(
            "Remote tasks cannot be archived. Archive them on the origin node.".to_string(),
        ));
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
        let child_ids: Vec<Uuid> = children.iter().map(|c| c.id).collect();
        subtasks_archived = Task::archive_many(pool, &child_ids).await?;
    }

    // Archive the main task
    let archived_task = Task::archive(pool, task.id).await?;

    // Gather cleanup data for background worktree cleanup
    let cleanup_args: Vec<WorktreeCleanup> = attempts
        .iter()
        .filter_map(|attempt| {
            attempt
                .container_ref
                .as_ref()
                .map(|worktree_path| WorktreeCleanup {
                    worktree_path: PathBuf::from(worktree_path),
                    git_repo_path: Some(project.git_repo_path.clone()),
                })
        })
        .collect();

    // Spawn background worktree cleanup task
    let task_id = task.id;
    tokio::spawn(async move {
        let span = tracing::info_span!("archive_worktree_cleanup", task_id = %task_id);
        let _enter = span.enter();

        tracing::info!(
            "Starting background worktree cleanup for archived task {} ({} worktrees)",
            task_id,
            cleanup_args.len()
        );

        if let Err(e) = WorktreeManager::batch_cleanup_worktrees(&cleanup_args).await {
            tracing::error!(
                "Background worktree cleanup failed for archived task {}: {}",
                task_id,
                e
            );
        } else {
            tracing::info!(
                "Background worktree cleanup completed for archived task {}",
                task_id
            );
        }
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

/// Unarchive a task.
///
/// This endpoint clears the `archived_at` timestamp, making the task visible again.
/// Note: Worktrees are not restored - a new attempt would need to be started.
pub async fn unarchive_task(
    Extension(task): Extension<Task>,
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<Task>>, ApiError> {
    let pool = &deployment.db().pool;

    // Remote tasks cannot be unarchived locally
    if task.is_remote {
        return Err(ApiError::BadRequest(
            "Remote tasks cannot be unarchived. Unarchive them on the origin node.".to_string(),
        ));
    }

    // Task not archived
    if task.archived_at.is_none() {
        return Err(ApiError::BadRequest("Task is not archived".to_string()));
    }

    // Unarchive the task
    let unarchived_task = Task::unarchive(pool, task.id).await?;

    Ok(ResponseJson(ApiResponse::success(unarchived_task)))
}

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

/// GET /api/tasks/{id}/labels - Get labels for a task
pub async fn get_task_labels(
    Extension(task): Extension<Task>,
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<Vec<Label>>>, ApiError> {
    let labels = Label::find_by_task_id(&deployment.db().pool, task.id).await?;
    Ok(ResponseJson(ApiResponse::success(labels)))
}

/// PUT /api/tasks/{id}/labels - Set labels for a task (replaces existing)
pub async fn set_task_labels(
    Extension(task): Extension<Task>,
    State(deployment): State<DeploymentImpl>,
    Json(payload): Json<SetTaskLabels>,
) -> Result<ResponseJson<ApiResponse<Vec<Label>>>, ApiError> {
    // Remote tasks don't support labels
    if task.is_remote {
        return Err(ApiError::BadRequest(
            "Labels are not supported for remote tasks".to_string(),
        ));
    }

    let labels = Label::set_task_labels(&deployment.db().pool, task.id, &payload.label_ids).await?;
    Ok(ResponseJson(ApiResponse::success(labels)))
}

pub fn router(deployment: &DeploymentImpl) -> Router<DeploymentImpl> {
    let task_actions_router = Router::new()
        .route("/", put(update_task))
        .route("/", delete(delete_task))
        .route("/share", post(share_task))
        .route("/archive", post(archive_task))
        .route("/unarchive", post(unarchive_task))
        .route("/children", get(get_task_children))
        .route("/labels", get(get_task_labels).put(set_task_labels))
        .route("/available-nodes", get(get_available_nodes))
        .route("/stream-connection-info", get(get_stream_connection_info));

    let task_id_router = Router::new()
        .route("/", get(get_task))
        .merge(task_actions_router)
        .layer(from_fn_with_state(deployment.clone(), load_task_middleware));

    let inner = Router::new()
        .route("/", get(get_tasks).post(create_task))
        .route("/stream/ws", get(stream_tasks_ws))
        .route("/create-and-start", post(create_task_and_start))
        .nest("/{task_id}", task_id_router);

    // mount under /projects/:project_id/tasks
    Router::new().nest("/tasks", inner)
}
