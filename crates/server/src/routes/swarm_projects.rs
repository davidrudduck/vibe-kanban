//! Swarm projects routes - proxies to the remote hive server.
//!
//! These routes provide access to swarm project management functionality
//! by proxying requests to the Hive's swarm projects API.

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    response::Json as ResponseJson,
    routing::{delete, get, post},
};
use remote::routes::swarm_projects::{
    ListSwarmProjectNodesResponse, ListSwarmProjectsResponse, SwarmProjectNodeResponse,
    SwarmProjectResponse,
};
use serde::Deserialize;
use services::services::remote_client::{
    CreateSwarmProjectRequest, LinkSwarmProjectNodeRequest, MergeSwarmProjectsRequest,
    UpdateSwarmProjectRequest,
};
use utils::response::ApiResponse;
use uuid::Uuid;

use crate::{DeploymentImpl, error::ApiError};

#[derive(Debug, Deserialize)]
pub struct ListSwarmProjectsQuery {
    pub organization_id: Uuid,
}

// =====================
// Handlers
// =====================

/// List all swarm projects for an organization.
pub async fn list_swarm_projects(
    State(deployment): State<DeploymentImpl>,
    Query(query): Query<ListSwarmProjectsQuery>,
) -> Result<ResponseJson<ApiResponse<ListSwarmProjectsResponse>>, ApiError> {
    let client = deployment.remote_client()?;
    let response = client.list_swarm_projects(query.organization_id).await?;
    Ok(ResponseJson(ApiResponse::success(response)))
}

/// Get a specific swarm project by ID.
pub async fn get_swarm_project(
    State(deployment): State<DeploymentImpl>,
    Path(project_id): Path<Uuid>,
) -> Result<ResponseJson<ApiResponse<SwarmProjectResponse>>, ApiError> {
    let client = deployment.remote_client()?;
    let response = client.get_swarm_project(project_id).await?;
    Ok(ResponseJson(ApiResponse::success(response)))
}

/// Create a new swarm project.
pub async fn create_swarm_project(
    State(deployment): State<DeploymentImpl>,
    Json(request): Json<CreateSwarmProjectRequest>,
) -> Result<ResponseJson<ApiResponse<SwarmProjectResponse>>, ApiError> {
    let client = deployment.remote_client()?;
    let response = client.create_swarm_project(&request).await?;
    Ok(ResponseJson(ApiResponse::success(response)))
}

/// Update an existing swarm project.
pub async fn update_swarm_project(
    State(deployment): State<DeploymentImpl>,
    Path(project_id): Path<Uuid>,
    Json(request): Json<UpdateSwarmProjectRequest>,
) -> Result<ResponseJson<ApiResponse<SwarmProjectResponse>>, ApiError> {
    let client = deployment.remote_client()?;
    let response = client.update_swarm_project(project_id, &request).await?;
    Ok(ResponseJson(ApiResponse::success(response)))
}

/// Delete a swarm project.
pub async fn delete_swarm_project(
    State(deployment): State<DeploymentImpl>,
    Path(project_id): Path<Uuid>,
) -> Result<ResponseJson<ApiResponse<()>>, ApiError> {
    let client = deployment.remote_client()?;
    client.delete_swarm_project(project_id).await?;
    Ok(ResponseJson(ApiResponse::success(())))
}

/// Merge two swarm projects.
pub async fn merge_swarm_projects(
    State(deployment): State<DeploymentImpl>,
    Path(project_id): Path<Uuid>,
    Json(request): Json<MergeSwarmProjectsRequest>,
) -> Result<ResponseJson<ApiResponse<SwarmProjectResponse>>, ApiError> {
    let client = deployment.remote_client()?;
    let response = client
        .merge_swarm_projects(project_id, request.source_id)
        .await?;
    Ok(ResponseJson(ApiResponse::success(response)))
}

/// List all node links for a swarm project.
pub async fn list_swarm_project_nodes(
    State(deployment): State<DeploymentImpl>,
    Path(project_id): Path<Uuid>,
) -> Result<ResponseJson<ApiResponse<ListSwarmProjectNodesResponse>>, ApiError> {
    let client = deployment.remote_client()?;
    let response = client.list_swarm_project_nodes(project_id).await?;
    Ok(ResponseJson(ApiResponse::success(response)))
}

/// Link a node project to a swarm project.
pub async fn link_swarm_project_node(
    State(deployment): State<DeploymentImpl>,
    Path(project_id): Path<Uuid>,
    Json(request): Json<LinkSwarmProjectNodeRequest>,
) -> Result<ResponseJson<ApiResponse<SwarmProjectNodeResponse>>, ApiError> {
    let client = deployment.remote_client()?;
    let response = client.link_swarm_project_node(project_id, &request).await?;
    Ok(ResponseJson(ApiResponse::success(response)))
}

/// Unlink a node from a swarm project.
pub async fn unlink_swarm_project_node(
    State(deployment): State<DeploymentImpl>,
    Path((project_id, node_id)): Path<(Uuid, Uuid)>,
) -> Result<ResponseJson<ApiResponse<()>>, ApiError> {
    let client = deployment.remote_client()?;
    client
        .unlink_swarm_project_node(project_id, node_id)
        .await?;
    Ok(ResponseJson(ApiResponse::success(())))
}

// =====================
// Router
// =====================

pub fn router() -> Router<DeploymentImpl> {
    Router::new()
        .route(
            "/swarm/projects",
            get(list_swarm_projects).post(create_swarm_project),
        )
        .route(
            "/swarm/projects/{project_id}",
            get(get_swarm_project)
                .patch(update_swarm_project)
                .delete(delete_swarm_project),
        )
        .route(
            "/swarm/projects/{project_id}/merge",
            post(merge_swarm_projects),
        )
        .route(
            "/swarm/projects/{project_id}/nodes",
            get(list_swarm_project_nodes).post(link_swarm_project_node),
        )
        .route(
            "/swarm/projects/{project_id}/nodes/{node_id}",
            delete(unlink_swarm_project_node),
        )
}
