//! Swarm templates routes - proxies to the remote hive server.
//!
//! These routes provide access to swarm template management functionality
//! by proxying requests to the Hive's swarm templates API.

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    response::Json as ResponseJson,
    routing::{get, post},
};
use remote::routes::swarm_templates::{ListSwarmTemplatesResponse, SwarmTemplateResponse};
use serde::Deserialize;
use services::services::remote_client::{
    CreateSwarmTemplateRequest, MergeSwarmTemplatesRequest, UpdateSwarmTemplateRequest,
};
use utils::response::ApiResponse;
use uuid::Uuid;

use crate::{DeploymentImpl, error::ApiError};

#[derive(Debug, Deserialize)]
pub struct ListSwarmTemplatesQuery {
    pub organization_id: Uuid,
}

// =====================
// Handlers
// =====================

/// List all swarm templates for an organization.
pub async fn list_swarm_templates(
    State(deployment): State<DeploymentImpl>,
    Query(query): Query<ListSwarmTemplatesQuery>,
) -> Result<ResponseJson<ApiResponse<ListSwarmTemplatesResponse>>, ApiError> {
    let client = deployment.remote_client()?;
    let response = client.list_swarm_templates(query.organization_id).await?;
    Ok(ResponseJson(ApiResponse::success(response)))
}

/// Get a specific swarm template by ID.
pub async fn get_swarm_template(
    State(deployment): State<DeploymentImpl>,
    Path(template_id): Path<Uuid>,
) -> Result<ResponseJson<ApiResponse<SwarmTemplateResponse>>, ApiError> {
    let client = deployment.remote_client()?;
    let response = client.get_swarm_template(template_id).await?;
    Ok(ResponseJson(ApiResponse::success(response)))
}

/// Create a new swarm template.
pub async fn create_swarm_template(
    State(deployment): State<DeploymentImpl>,
    Json(request): Json<CreateSwarmTemplateRequest>,
) -> Result<ResponseJson<ApiResponse<SwarmTemplateResponse>>, ApiError> {
    let client = deployment.remote_client()?;
    let response = client.create_swarm_template(&request).await?;
    Ok(ResponseJson(ApiResponse::success(response)))
}

/// Update an existing swarm template.
pub async fn update_swarm_template(
    State(deployment): State<DeploymentImpl>,
    Path(template_id): Path<Uuid>,
    Json(request): Json<UpdateSwarmTemplateRequest>,
) -> Result<ResponseJson<ApiResponse<SwarmTemplateResponse>>, ApiError> {
    let client = deployment.remote_client()?;
    let response = client.update_swarm_template(template_id, &request).await?;
    Ok(ResponseJson(ApiResponse::success(response)))
}

/// Delete a swarm template.
pub async fn delete_swarm_template(
    State(deployment): State<DeploymentImpl>,
    Path(template_id): Path<Uuid>,
) -> Result<ResponseJson<ApiResponse<()>>, ApiError> {
    let client = deployment.remote_client()?;
    client.delete_swarm_template(template_id).await?;
    Ok(ResponseJson(ApiResponse::success(())))
}

/// Merge two swarm templates.
pub async fn merge_swarm_templates(
    State(deployment): State<DeploymentImpl>,
    Path(template_id): Path<Uuid>,
    Json(request): Json<MergeSwarmTemplatesRequest>,
) -> Result<ResponseJson<ApiResponse<SwarmTemplateResponse>>, ApiError> {
    let client = deployment.remote_client()?;
    let response = client
        .merge_swarm_templates(template_id, request.source_id)
        .await?;
    Ok(ResponseJson(ApiResponse::success(response)))
}

// =====================
// Router
// =====================

pub fn router() -> Router<DeploymentImpl> {
    Router::new()
        .route(
            "/swarm/templates",
            get(list_swarm_templates).post(create_swarm_template),
        )
        .route(
            "/swarm/templates/{template_id}",
            get(get_swarm_template)
                .patch(update_swarm_template)
                .delete(delete_swarm_template),
        )
        .route(
            "/swarm/templates/{template_id}/merge",
            post(merge_swarm_templates),
        )
}
