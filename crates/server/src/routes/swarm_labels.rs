//! Swarm labels routes - proxies to the remote hive server.
//!
//! These routes provide access to swarm label management functionality
//! by proxying requests to the Hive's swarm labels API.

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    response::Json as ResponseJson,
    routing::{get, post},
};
use remote::routes::swarm_labels::{
    ListSwarmLabelsResponse, MergeLabelsResult, SwarmLabelResponse,
};
use serde::Deserialize;
use services::services::remote_client::{
    CreateSwarmLabelRequest, MergeSwarmLabelsRequest, PromoteToSwarmRequest,
    UpdateSwarmLabelRequest,
};
use utils::response::ApiResponse;
use uuid::Uuid;

use crate::{DeploymentImpl, error::ApiError};

#[derive(Debug, Deserialize)]
pub struct ListSwarmLabelsQuery {
    pub organization_id: Uuid,
}

// =====================
// Handlers
// =====================

/// List all swarm labels for an organization.
pub async fn list_swarm_labels(
    State(deployment): State<DeploymentImpl>,
    Query(query): Query<ListSwarmLabelsQuery>,
) -> Result<ResponseJson<ApiResponse<ListSwarmLabelsResponse>>, ApiError> {
    let client = deployment.remote_client()?;
    let response = client.list_swarm_labels(query.organization_id).await?;
    Ok(ResponseJson(ApiResponse::success(response)))
}

/// Get a specific swarm label by ID.
pub async fn get_swarm_label(
    State(deployment): State<DeploymentImpl>,
    Path(label_id): Path<Uuid>,
) -> Result<ResponseJson<ApiResponse<SwarmLabelResponse>>, ApiError> {
    let client = deployment.remote_client()?;
    let response = client.get_swarm_label(label_id).await?;
    Ok(ResponseJson(ApiResponse::success(response)))
}

/// Create a new swarm label.
pub async fn create_swarm_label(
    State(deployment): State<DeploymentImpl>,
    Json(request): Json<CreateSwarmLabelRequest>,
) -> Result<ResponseJson<ApiResponse<SwarmLabelResponse>>, ApiError> {
    let client = deployment.remote_client()?;
    let response = client.create_swarm_label(&request).await?;
    Ok(ResponseJson(ApiResponse::success(response)))
}

/// Update an existing swarm label.
pub async fn update_swarm_label(
    State(deployment): State<DeploymentImpl>,
    Path(label_id): Path<Uuid>,
    Json(request): Json<UpdateSwarmLabelRequest>,
) -> Result<ResponseJson<ApiResponse<SwarmLabelResponse>>, ApiError> {
    let client = deployment.remote_client()?;
    let response = client.update_swarm_label(label_id, &request).await?;
    Ok(ResponseJson(ApiResponse::success(response)))
}

/// Delete a swarm label.
pub async fn delete_swarm_label(
    State(deployment): State<DeploymentImpl>,
    Path(label_id): Path<Uuid>,
) -> Result<ResponseJson<ApiResponse<()>>, ApiError> {
    let client = deployment.remote_client()?;
    client.delete_swarm_label(label_id).await?;
    Ok(ResponseJson(ApiResponse::success(())))
}

/// Merge two swarm labels.
pub async fn merge_swarm_labels(
    State(deployment): State<DeploymentImpl>,
    Path(label_id): Path<Uuid>,
    Json(request): Json<MergeSwarmLabelsRequest>,
) -> Result<ResponseJson<ApiResponse<MergeLabelsResult>>, ApiError> {
    let client = deployment.remote_client()?;
    let response = client
        .merge_swarm_labels(label_id, request.source_id)
        .await?;
    Ok(ResponseJson(ApiResponse::success(response)))
}

/// Promote a project-scoped label to a swarm label.
pub async fn promote_to_swarm(
    State(deployment): State<DeploymentImpl>,
    Json(request): Json<PromoteToSwarmRequest>,
) -> Result<ResponseJson<ApiResponse<SwarmLabelResponse>>, ApiError> {
    let client = deployment.remote_client()?;
    let response = client.promote_label_to_swarm(request.label_id).await?;
    Ok(ResponseJson(ApiResponse::success(response)))
}

// =====================
// Router
// =====================

pub fn router() -> Router<DeploymentImpl> {
    Router::new()
        .route(
            "/swarm/labels",
            get(list_swarm_labels).post(create_swarm_label),
        )
        .route(
            "/swarm/labels/{label_id}",
            get(get_swarm_label)
                .patch(update_swarm_label)
                .delete(delete_swarm_label),
        )
        .route("/swarm/labels/{label_id}/merge", post(merge_swarm_labels))
        .route("/swarm/labels/promote", post(promote_to_swarm))
}
