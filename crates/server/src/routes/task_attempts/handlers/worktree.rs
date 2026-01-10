//! Worktree-related handlers: file browser, cleanup, path access, build artifact purge.

use std::path::PathBuf;

use axum::{
    Extension,
    extract::{
        Path, Query, State,
        ws::{WebSocket, WebSocketUpgrade},
    },
    response::{IntoResponse, Json as ResponseJson},
};
use db::models::{
    execution_process::ExecutionProcess,
    log_entry::{CreateLogEntry, DbLogEntry},
    project::Project,
    task::Task,
    task_attempt::TaskAttempt,
};
use deployment::Deployment;
use services::services::{
    container::ContainerService,
    filesystem::{DirectoryListResponse, FileContentResponse, FilesystemError},
    worktree_manager::{PurgeResult, WorktreeCleanup, WorktreeManager},
};
use sqlx::Error as SqlxError;
use utils::response::ApiResponse;
use utils::unified_log::OutputType;

use crate::routes::task_attempts::types::{DiffStreamQuery, ListFilesQuery, WorktreePathResponse};
use crate::routes::task_attempts::util::ensure_worktree_path;
use crate::{
    DeploymentImpl, error::ApiError, middleware::RemoteTaskAttemptContext,
    proxy::check_remote_task_attempt_proxy,
};

// ============================================================================
// Diff Streaming
// ============================================================================

#[axum::debug_handler]
pub async fn stream_task_attempt_diff_ws(
    ws: WebSocketUpgrade,
    Query(params): Query<DiffStreamQuery>,
    Extension(task_attempt): Extension<TaskAttempt>,
    State(deployment): State<DeploymentImpl>,
) -> impl IntoResponse {
    let stats_only = params.stats_only;
    ws.on_upgrade(move |socket| async move {
        if let Err(e) =
            handle_task_attempt_diff_ws(socket, deployment, task_attempt, stats_only).await
        {
            tracing::warn!("diff WS closed: {}", e);
        }
    })
}

async fn handle_task_attempt_diff_ws(
    socket: WebSocket,
    deployment: DeploymentImpl,
    task_attempt: TaskAttempt,
    stats_only: bool,
) -> anyhow::Result<()> {
    use crate::ws_util::{WsKeepAlive, run_ws_stream};
    use futures_util::TryStreamExt;
    use utils::log_msg::LogMsg;

    let stream = deployment
        .container()
        .stream_diff(&task_attempt, stats_only)
        .await?;

    let stream = stream.map_ok(|msg: LogMsg| msg.to_ws_message_unchecked());

    // Use run_ws_stream for proper keep-alive handling
    run_ws_stream(socket, stream, WsKeepAlive::for_execution_streams()).await
}

// ============================================================================
// File Browser Endpoints
// ============================================================================

/// List files and directories within a task attempt's worktree
pub async fn list_worktree_files(
    Extension(task_attempt): Extension<TaskAttempt>,
    remote_ctx: Option<Extension<RemoteTaskAttemptContext>>,
    State(deployment): State<DeploymentImpl>,
    Query(query): Query<ListFilesQuery>,
) -> Result<ResponseJson<ApiResponse<DirectoryListResponse>>, ApiError> {
    // Check if this is a remote task attempt that should be proxied
    if let Some(proxy_info) = check_remote_task_attempt_proxy(remote_ctx.as_ref().map(|e| &e.0))? {
        tracing::debug!(
            node_id = %proxy_info.node_id,
            shared_task_id = %proxy_info.target_id,
            "Proxying list_worktree_files to remote node"
        );

        let path = match &query.path {
            Some(p) => format!(
                "/task-attempts/by-task-id/{}/files?path={}",
                proxy_info.target_id,
                urlencoding::encode(p)
            ),
            None => format!("/task-attempts/by-task-id/{}/files", proxy_info.target_id),
        };
        let response: ApiResponse<DirectoryListResponse> = deployment
            .node_proxy_client()
            .proxy_get(&proxy_info.node_url, &path, proxy_info.node_id)
            .await?;

        return Ok(ResponseJson(response));
    }

    let worktree_path = ensure_worktree_path(&deployment, &task_attempt).await?;

    match deployment
        .filesystem()
        .list_directory_within(&worktree_path, query.path.as_deref())
        .await
    {
        Ok(response) => Ok(ResponseJson(ApiResponse::success(response))),
        Err(FilesystemError::DirectoryDoesNotExist) => {
            Ok(ResponseJson(ApiResponse::error("Directory does not exist")))
        }
        Err(FilesystemError::PathIsNotDirectory) => {
            Ok(ResponseJson(ApiResponse::error("Path is not a directory")))
        }
        Err(FilesystemError::PathTraversalNotAllowed) => Ok(ResponseJson(ApiResponse::error(
            "Path traversal not allowed",
        ))),
        Err(FilesystemError::Io(e)) => {
            tracing::error!("Failed to list worktree directory: {}", e);
            Ok(ResponseJson(ApiResponse::error(&format!(
                "Failed to list directory: {}",
                e
            ))))
        }
        Err(e) => {
            tracing::error!("Unexpected error listing worktree: {}", e);
            Ok(ResponseJson(ApiResponse::error(&e.to_string())))
        }
    }
}

/// Read file content from a task attempt's worktree
pub async fn read_worktree_file(
    Extension(task_attempt): Extension<TaskAttempt>,
    remote_ctx: Option<Extension<RemoteTaskAttemptContext>>,
    State(deployment): State<DeploymentImpl>,
    Path((_, file_path)): Path<(String, String)>,
) -> Result<ResponseJson<ApiResponse<FileContentResponse>>, ApiError> {
    // Check if this is a remote task attempt that should be proxied
    if let Some(proxy_info) = check_remote_task_attempt_proxy(remote_ctx.as_ref().map(|e| &e.0))? {
        tracing::debug!(
            node_id = %proxy_info.node_id,
            shared_task_id = %proxy_info.target_id,
            file_path = %file_path,
            "Proxying read_worktree_file to remote node"
        );

        let path = format!(
            "/task-attempts/by-task-id/{}/files/{}",
            proxy_info.target_id, file_path
        );
        let response: ApiResponse<FileContentResponse> = deployment
            .node_proxy_client()
            .proxy_get(&proxy_info.node_url, &path, proxy_info.node_id)
            .await?;

        return Ok(ResponseJson(response));
    }

    let worktree_path = ensure_worktree_path(&deployment, &task_attempt).await?;

    match deployment
        .filesystem()
        .read_file_within(&worktree_path, &file_path, None)
        .await
    {
        Ok(response) => Ok(ResponseJson(ApiResponse::success(response))),
        Err(FilesystemError::FileDoesNotExist) => {
            Ok(ResponseJson(ApiResponse::error("File does not exist")))
        }
        Err(FilesystemError::PathIsNotFile) => {
            Ok(ResponseJson(ApiResponse::error("Path is not a file")))
        }
        Err(FilesystemError::PathTraversalNotAllowed) => Ok(ResponseJson(ApiResponse::error(
            "Path traversal not allowed",
        ))),
        Err(FilesystemError::FileIsBinary) => Ok(ResponseJson(ApiResponse::error(
            "Cannot display binary file",
        ))),
        Err(FilesystemError::FileTooLarge {
            max_bytes,
            actual_bytes,
        }) => Ok(ResponseJson(ApiResponse::error(&format!(
            "File too large ({} bytes, max {} bytes)",
            actual_bytes, max_bytes
        )))),
        Err(FilesystemError::Io(e)) => {
            tracing::error!("Failed to read worktree file: {}", e);
            Ok(ResponseJson(ApiResponse::error(&format!(
                "Failed to read file: {}",
                e
            ))))
        }
        Err(e) => {
            tracing::error!("Unexpected error reading file: {}", e);
            Ok(ResponseJson(ApiResponse::error(&e.to_string())))
        }
    }
}

// ============================================================================
// Worktree Path and Cleanup
// ============================================================================

/// Get the worktree path for a task attempt
///
/// GET /api/task-attempts/{id}/worktree-path
///
/// Returns the absolute path to the worktree directory for the given task attempt.
/// This is useful for opening a terminal in the worktree directory.
pub async fn get_worktree_path(
    Extension(task_attempt): Extension<TaskAttempt>,
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<WorktreePathResponse>>, ApiError> {
    let worktree_path = ensure_worktree_path(&deployment, &task_attempt).await?;
    let path_string = worktree_path
        .to_str()
        .ok_or_else(|| ApiError::BadRequest("Invalid worktree path".to_string()))?
        .to_string();

    Ok(ResponseJson(ApiResponse::success(WorktreePathResponse {
        path: path_string,
    })))
}

/// Clean up the worktree for a task attempt
///
/// POST /api/task-attempts/{id}/cleanup
///
/// Deletes the worktree filesystem and marks the attempt as cleaned up in the database.
/// Returns 409 Conflict if there are running processes.
/// Returns 200 OK if already cleaned up (idempotent).
pub async fn cleanup_worktree(
    Extension(task_attempt): Extension<TaskAttempt>,
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<()>>, ApiError> {
    let pool = &deployment.db().pool;

    // Already cleaned up - return success (idempotent)
    if task_attempt.worktree_deleted {
        return Ok(ResponseJson(ApiResponse::success(())));
    }

    // Check for running processes
    if deployment
        .container()
        .has_running_processes_for_attempt(task_attempt.id)
        .await?
    {
        return Err(ApiError::Conflict(
            "Task attempt has running execution processes. Stop them first.".to_string(),
        ));
    }

    // Get the worktree path
    let worktree_path = match &task_attempt.container_ref {
        Some(path) => PathBuf::from(path),
        None => {
            // No worktree path set, just mark as deleted
            TaskAttempt::mark_worktree_deleted(pool, task_attempt.id).await?;
            return Ok(ResponseJson(ApiResponse::success(())));
        }
    };

    // Get git repo path for proper cleanup
    let task = Task::find_by_id(pool, task_attempt.task_id)
        .await?
        .ok_or_else(|| ApiError::Database(SqlxError::RowNotFound))?;
    let project = Project::find_by_id(pool, task.project_id)
        .await?
        .ok_or_else(|| ApiError::Database(SqlxError::RowNotFound))?;

    // Clean up the worktree
    let cleanup = WorktreeCleanup::new(worktree_path, Some(project.git_repo_path.clone()));
    if let Err(e) = WorktreeManager::cleanup_worktree(&cleanup).await {
        tracing::error!(
            "Failed to cleanup worktree for attempt {}: {}",
            task_attempt.id,
            e
        );
        return Err(e.into());
    }

    // Mark worktree as deleted in database
    TaskAttempt::mark_worktree_deleted(pool, task_attempt.id).await?;

    // Log cleanup to attempt's conversation
    if let Ok(Some(execution)) =
        ExecutionProcess::find_latest_for_attempt(pool, task_attempt.id).await
    {
        let log_content = format!(
            "[System] Worktree deleted: {}",
            task_attempt.container_ref.as_deref().unwrap_or("unknown")
        );
        let _ = DbLogEntry::create(
            pool,
            CreateLogEntry::new(execution.id, OutputType::System, log_content),
        )
        .await;
    }

    tracing::info!(
        "Successfully cleaned up worktree for attempt {}",
        task_attempt.id
    );

    Ok(ResponseJson(ApiResponse::success(())))
}

/// Purge build artifacts from a task attempt's worktree
///
/// POST /api/task-attempts/{id}/purge
///
/// Removes build artifacts (target/, node_modules/, .next/, dist/, build/) from the worktree
/// without deleting the worktree itself. Returns the amount of disk space freed.
pub async fn purge_build_artifacts(
    Extension(task_attempt): Extension<TaskAttempt>,
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<PurgeResult>>, ApiError> {
    // Check if worktree exists
    if task_attempt.container_ref.is_none() {
        // No worktree path set, nothing to purge
        return Ok(ResponseJson(ApiResponse::success(PurgeResult {
            freed_bytes: 0,
            purged_dirs: Vec::new(),
        })));
    }

    // Ensure the worktree exists (may recreate if missing)
    let worktree_path = ensure_worktree_path(&deployment, &task_attempt).await?;

    // Purge build artifacts
    let result = WorktreeManager::purge_build_artifacts(&worktree_path).await?;

    // Log purge result to attempt's conversation
    let pool = &deployment.db().pool;
    if let Ok(Some(execution)) =
        ExecutionProcess::find_latest_for_attempt(pool, task_attempt.id).await
    {
        let log_content = if result.purged_dirs.is_empty() {
            "[System] Purge: No build artifacts found to remove".to_string()
        } else {
            let freed_mb = result.freed_bytes as f64 / (1024.0 * 1024.0);
            format!(
                "[System] Purged build artifacts: {} - freed {:.1} MB",
                result.purged_dirs.join(", "),
                freed_mb
            )
        };
        let _ = DbLogEntry::create(
            pool,
            CreateLogEntry::new(execution.id, OutputType::System, log_content),
        )
        .await;
    }

    tracing::info!(
        "Purged {} bytes of build artifacts from attempt {}",
        result.freed_bytes,
        task_attempt.id
    );

    Ok(ResponseJson(ApiResponse::success(result)))
}
