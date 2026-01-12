//! WebSocket and streaming handlers: stream_tasks_ws, get_available_nodes, get_stream_connection_info.

use axum::{
    Extension,
    extract::{
        Query, State,
        ws::{WebSocket, WebSocketUpgrade},
    },
    response::{IntoResponse, Json as ResponseJson},
};
use db::models::{project::Project, task::Task};
use deployment::Deployment;
use futures_util::TryStreamExt;
use remote::routes::{projects::ListProjectNodesResponse, tasks::TaskStreamConnectionInfoResponse};
use utils::response::ApiResponse;
use uuid::Uuid;

use crate::routes::tasks::types::TaskQuery;
use crate::{
    DeploymentImpl,
    error::ApiError,
    ws_util::{WsKeepAlive, run_ws_stream},
};

// ============================================================================
// WebSocket Task Stream
// ============================================================================

/// WebSocket endpoint for streaming task updates.
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

/// Internal handler for WebSocket task streaming.
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

// ============================================================================
// Available Nodes
// ============================================================================

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

// ============================================================================
// Stream Connection Info
// ============================================================================

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
