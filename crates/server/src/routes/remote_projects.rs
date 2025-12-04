//! Proxy routes for remote project task management.
//!
//! These routes proxy task operations to remote nodes through the Hive,
//! allowing users to manage tasks on remote projects from any node's frontend.

use axum::{
    Json, Router,
    extract::{Path, State},
    response::Json as ResponseJson,
    routing::{delete, get, patch, post},
};
use deployment::Deployment;
use remote::routes::tasks::{
    BulkSharedTasksResponse, CreateSharedTaskRequest, SharedTaskResponse, UpdateSharedTaskRequest,
};
use utils::api::projects::RemoteProject;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use utils::response::ApiResponse;
use uuid::Uuid;

use crate::{DeploymentImpl, error::ApiError};

/// Response for remote project info
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct RemoteProjectInfo {
    pub project_id: Uuid,
    pub project_name: String,
    pub node_id: Uuid,
    pub node_name: String,
    pub node_status: String,
    pub git_repo_path: String,
    pub default_branch: String,
}

/// Request to create a task on a remote project
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(export)]
pub struct CreateRemoteTaskRequest {
    pub title: String,
    pub description: Option<String>,
    pub assignee_user_id: Option<Uuid>,
}

/// Request to update a task on a remote project
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(export)]
pub struct UpdateRemoteTaskRequest {
    pub title: Option<String>,
    pub description: Option<String>,
    pub status: Option<String>,
    pub version: Option<i64>,
}

/// Request to assign a task on a remote project
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(export)]
pub struct AssignRemoteTaskRequest {
    pub new_assignee_user_id: Option<Uuid>,
    pub version: Option<i64>,
}

pub fn router(deployment: &DeploymentImpl) -> Router<DeploymentImpl> {
    let _ = deployment; // Mark as used
    Router::new()
        .route(
            "/remote-projects/{project_id}",
            get(get_remote_project_info),
        )
        .route(
            "/remote-projects/{project_id}/hive-details",
            get(get_remote_project_from_hive),
        )
        .route(
            "/remote-projects/{project_id}/tasks",
            get(get_remote_project_tasks),
        )
        .route(
            "/remote-projects/{project_id}/tasks",
            post(create_remote_project_task),
        )
        .route(
            "/remote-projects/{project_id}/tasks/{task_id}",
            patch(update_remote_project_task),
        )
        .route(
            "/remote-projects/{project_id}/tasks/{task_id}",
            delete(delete_remote_project_task),
        )
        .route(
            "/remote-projects/{project_id}/tasks/{task_id}/assign",
            post(assign_remote_task),
        )
}

/// Get information about a remote project
///
/// This looks up the project in the local cache to get node/project info.
pub async fn get_remote_project_info(
    State(deployment): State<DeploymentImpl>,
    Path(project_id): Path<Uuid>,
) -> Result<ResponseJson<ApiResponse<RemoteProjectInfo>>, ApiError> {
    use db::models::cached_node_project::CachedNodeProjectWithNode;

    let pool = &deployment.db().pool;

    // Find the cached project by its remote project_id
    // We need to look through cached projects to find one with matching project_id
    let all_cached = CachedNodeProjectWithNode::list_all(pool).await?;

    let cached_project = all_cached
        .into_iter()
        .find(|p| p.project_id == project_id)
        .ok_or_else(|| ApiError::BadRequest("Remote project not found in cache".to_string()))?;

    Ok(ResponseJson(ApiResponse::success(RemoteProjectInfo {
        project_id: cached_project.project_id,
        project_name: cached_project.project_name,
        node_id: cached_project.node_id,
        node_name: cached_project.node_name,
        node_status: cached_project.node_status.to_string(),
        git_repo_path: cached_project.git_repo_path,
        default_branch: cached_project.default_branch,
    })))
}

/// Get remote project details directly from the Hive
///
/// This fetches project details from the Hive API, not from the local cache.
/// Use this when you need the most up-to-date information from the source.
pub async fn get_remote_project_from_hive(
    State(deployment): State<DeploymentImpl>,
    Path(project_id): Path<Uuid>,
) -> Result<ResponseJson<ApiResponse<RemoteProject>>, ApiError> {
    let client = deployment.remote_client()?;

    let remote_project = client.get_project(project_id).await?;

    Ok(ResponseJson(ApiResponse::success(remote_project)))
}

/// Get all tasks for a remote project
///
/// Proxies the request through the Hive to get the task list.
pub async fn get_remote_project_tasks(
    State(deployment): State<DeploymentImpl>,
    Path(project_id): Path<Uuid>,
) -> Result<ResponseJson<ApiResponse<BulkSharedTasksResponse>>, ApiError> {
    let remote_client = deployment.remote_client()?;
    let tasks = remote_client.fetch_bulk_snapshot(project_id).await?;
    Ok(ResponseJson(ApiResponse::success(tasks)))
}

/// Create a task on a remote project
///
/// Proxies the create request through the Hive.
pub async fn create_remote_project_task(
    State(deployment): State<DeploymentImpl>,
    Path(project_id): Path<Uuid>,
    Json(payload): Json<CreateRemoteTaskRequest>,
) -> Result<ResponseJson<ApiResponse<SharedTaskResponse>>, ApiError> {
    let remote_client = deployment.remote_client()?;

    let request = CreateSharedTaskRequest {
        project_id,
        title: payload.title,
        description: payload.description,
        status: None, // Default to Todo on the Hive
        assignee_user_id: payload.assignee_user_id,
    };

    let response = remote_client.create_shared_task(&request).await?;

    Ok(ResponseJson(ApiResponse::success(response)))
}

/// Update a task on a remote project
///
/// Proxies the update request through the Hive.
pub async fn update_remote_project_task(
    State(deployment): State<DeploymentImpl>,
    Path((_project_id, task_id)): Path<(Uuid, Uuid)>,
    Json(payload): Json<UpdateRemoteTaskRequest>,
) -> Result<ResponseJson<ApiResponse<SharedTaskResponse>>, ApiError> {
    use remote::db::tasks::TaskStatus;

    let remote_client = deployment.remote_client()?;

    // Parse the status string to TaskStatus enum if provided
    let status = payload
        .status
        .as_ref()
        .map(|s| {
            serde_json::from_value::<TaskStatus>(serde_json::Value::String(s.clone()))
                .map_err(|_| ApiError::BadRequest(format!("Invalid task status: {}", s)))
        })
        .transpose()?;

    let request = UpdateSharedTaskRequest {
        title: payload.title,
        description: payload.description,
        status,
        version: payload.version,
    };

    let response = remote_client.update_shared_task(task_id, &request).await?;

    Ok(ResponseJson(ApiResponse::success(response)))
}

/// Delete a task on a remote project
///
/// Proxies the delete request through the Hive.
pub async fn delete_remote_project_task(
    State(deployment): State<DeploymentImpl>,
    Path((_project_id, task_id)): Path<(Uuid, Uuid)>,
) -> Result<ResponseJson<ApiResponse<SharedTaskResponse>>, ApiError> {
    use remote::routes::tasks::DeleteSharedTaskRequest;

    let remote_client = deployment.remote_client()?;

    let request = DeleteSharedTaskRequest { version: None };

    let response = remote_client.delete_shared_task(task_id, &request).await?;

    Ok(ResponseJson(ApiResponse::success(response)))
}

/// Assign a task on a remote project
///
/// Proxies the assign request through the Hive.
pub async fn assign_remote_task(
    State(deployment): State<DeploymentImpl>,
    Path((_project_id, task_id)): Path<(Uuid, Uuid)>,
    Json(payload): Json<AssignRemoteTaskRequest>,
) -> Result<ResponseJson<ApiResponse<SharedTaskResponse>>, ApiError> {
    use remote::routes::tasks::AssignSharedTaskRequest;

    let remote_client = deployment.remote_client()?;

    let request = AssignSharedTaskRequest {
        new_assignee_user_id: payload.new_assignee_user_id,
        version: payload.version,
    };

    let response = remote_client.assign_shared_task(task_id, &request).await?;

    Ok(ResponseJson(ApiResponse::success(response)))
}
