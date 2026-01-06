//! Routes for managing swarm projects in the Hive.
//!
//! Swarm projects are explicitly linked projects that can have multiple node projects
//! attached, enabling task sharing across nodes.

use axum::{
    Json, Router,
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    routing::{delete, get, post},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::instrument;
use uuid::Uuid;

use super::{error::ErrorResponse, organization_members::ensure_member_access};
use crate::{
    AppState,
    auth::RequestContext,
    db::{
        node_local_projects::NodeLocalProjectRepository,
        swarm_projects::{
            CreateSwarmProjectData, LinkSwarmProjectNodeData, SwarmProject, SwarmProjectError,
            SwarmProjectNode, SwarmProjectRepository, SwarmProjectWithNodes, UpdateSwarmProjectData,
        },
    },
};

// =====================
// Query & Request Types
// =====================

#[derive(Debug, Deserialize)]
pub struct ListSwarmProjectsQuery {
    pub organization_id: Uuid,
}

#[derive(Debug, Deserialize)]
pub struct CreateSwarmProjectRequest {
    pub organization_id: Uuid,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Debug, Deserialize)]
pub struct UpdateSwarmProjectRequest {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub metadata: Option<Value>,
}

#[derive(Debug, Deserialize)]
pub struct MergeSwarmProjectsRequest {
    /// The swarm project to merge into this one (will be deleted)
    pub source_id: Uuid,
}

#[derive(Debug, Deserialize)]
pub struct LinkNodeRequest {
    pub node_id: Uuid,
    pub local_project_id: Uuid,
    pub git_repo_path: String,
    #[serde(default)]
    pub os_type: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UnlinkNodeRequest {
    pub node_id: Uuid,
}

// =====================
// Response Types
// =====================

#[derive(Debug, Serialize, Deserialize)]
pub struct SwarmProjectResponse {
    pub project: SwarmProject,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ListSwarmProjectsResponse {
    pub projects: Vec<SwarmProjectWithNodes>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SwarmProjectNodeResponse {
    pub link: SwarmProjectNode,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ListSwarmProjectNodesResponse {
    pub nodes: Vec<SwarmProjectNode>,
}

// =====================
// Router
// =====================

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/swarm/projects", get(list_swarm_projects).post(create_swarm_project))
        .route(
            "/swarm/projects/{project_id}",
            get(get_swarm_project)
                .patch(update_swarm_project)
                .delete(delete_swarm_project),
        )
        .route("/swarm/projects/{project_id}/merge", post(merge_swarm_projects))
        .route(
            "/swarm/projects/{project_id}/nodes",
            get(list_swarm_project_nodes).post(link_node),
        )
        .route(
            "/swarm/projects/{project_id}/nodes/{node_id}",
            delete(unlink_node),
        )
}

// =====================
// Handlers
// =====================

#[instrument(
    name = "swarm_projects.list",
    skip(state, ctx, params),
    fields(org_id = %params.organization_id, user_id = %ctx.user.id)
)]
async fn list_swarm_projects(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
    Query(params): Query<ListSwarmProjectsQuery>,
) -> Result<Json<ListSwarmProjectsResponse>, ErrorResponse> {
    ensure_member_access(state.pool(), params.organization_id, ctx.user.id).await?;

    let projects = SwarmProjectRepository::list_with_nodes_count(state.pool(), params.organization_id)
        .await
        .map_err(|error| {
            tracing::error!(?error, "failed to list swarm projects");
            ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "failed to list swarm projects")
        })?;

    Ok(Json(ListSwarmProjectsResponse { projects }))
}

#[instrument(
    name = "swarm_projects.get",
    skip(state, ctx),
    fields(project_id = %project_id, user_id = %ctx.user.id)
)]
async fn get_swarm_project(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
    Path(project_id): Path<Uuid>,
) -> Result<Json<SwarmProjectResponse>, ErrorResponse> {
    let project = SwarmProjectRepository::find_by_id(state.pool(), project_id)
        .await
        .map_err(|error| {
            tracing::error!(?error, %project_id, "failed to get swarm project");
            ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "failed to get swarm project")
        })?
        .ok_or_else(|| ErrorResponse::new(StatusCode::NOT_FOUND, "swarm project not found"))?;

    ensure_member_access(state.pool(), project.organization_id, ctx.user.id).await?;

    Ok(Json(SwarmProjectResponse { project }))
}

#[instrument(
    name = "swarm_projects.create",
    skip(state, ctx, payload),
    fields(user_id = %ctx.user.id, org_id = %payload.organization_id)
)]
async fn create_swarm_project(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
    Json(payload): Json<CreateSwarmProjectRequest>,
) -> Result<Json<SwarmProjectResponse>, ErrorResponse> {
    ensure_member_access(state.pool(), payload.organization_id, ctx.user.id).await?;

    let mut tx = state.pool().begin().await.map_err(|error| {
        tracing::error!(?error, "failed to start transaction");
        ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "internal server error")
    })?;

    let project = SwarmProjectRepository::create(
        &mut tx,
        CreateSwarmProjectData {
            organization_id: payload.organization_id,
            name: payload.name,
            description: payload.description,
            metadata: payload.metadata,
        },
    )
    .await
    .map_err(|error| match error {
        SwarmProjectError::NameConflict => {
            ErrorResponse::new(StatusCode::CONFLICT, "swarm project name already exists")
        }
        SwarmProjectError::InvalidMetadata => {
            ErrorResponse::new(StatusCode::BAD_REQUEST, "metadata must be a JSON object")
        }
        _ => {
            tracing::error!(?error, "failed to create swarm project");
            ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "failed to create swarm project")
        }
    })?;

    tx.commit().await.map_err(|error| {
        tracing::error!(?error, "failed to commit transaction");
        ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "internal server error")
    })?;

    Ok(Json(SwarmProjectResponse { project }))
}

#[instrument(
    name = "swarm_projects.update",
    skip(state, ctx, payload),
    fields(project_id = %project_id, user_id = %ctx.user.id)
)]
async fn update_swarm_project(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
    Path(project_id): Path<Uuid>,
    Json(payload): Json<UpdateSwarmProjectRequest>,
) -> Result<Json<SwarmProjectResponse>, ErrorResponse> {
    // Get the project to verify org access
    let existing = SwarmProjectRepository::find_by_id(state.pool(), project_id)
        .await
        .map_err(|error| {
            tracing::error!(?error, %project_id, "failed to get swarm project");
            ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "failed to get swarm project")
        })?
        .ok_or_else(|| ErrorResponse::new(StatusCode::NOT_FOUND, "swarm project not found"))?;

    ensure_member_access(state.pool(), existing.organization_id, ctx.user.id).await?;

    let mut tx = state.pool().begin().await.map_err(|error| {
        tracing::error!(?error, "failed to start transaction");
        ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "internal server error")
    })?;

    let project = SwarmProjectRepository::update(
        &mut tx,
        project_id,
        UpdateSwarmProjectData {
            name: payload.name,
            description: payload.description,
            metadata: payload.metadata,
        },
    )
    .await
    .map_err(|error| match error {
        SwarmProjectError::NotFound => {
            ErrorResponse::new(StatusCode::NOT_FOUND, "swarm project not found")
        }
        SwarmProjectError::NameConflict => {
            ErrorResponse::new(StatusCode::CONFLICT, "swarm project name already exists")
        }
        SwarmProjectError::InvalidMetadata => {
            ErrorResponse::new(StatusCode::BAD_REQUEST, "metadata must be a JSON object")
        }
        _ => {
            tracing::error!(?error, "failed to update swarm project");
            ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "failed to update swarm project")
        }
    })?;

    tx.commit().await.map_err(|error| {
        tracing::error!(?error, "failed to commit transaction");
        ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "internal server error")
    })?;

    Ok(Json(SwarmProjectResponse { project }))
}

#[instrument(
    name = "swarm_projects.delete",
    skip(state, ctx),
    fields(project_id = %project_id, user_id = %ctx.user.id)
)]
async fn delete_swarm_project(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
    Path(project_id): Path<Uuid>,
) -> Result<StatusCode, ErrorResponse> {
    // Get the project to verify org access
    let existing = SwarmProjectRepository::find_by_id(state.pool(), project_id)
        .await
        .map_err(|error| {
            tracing::error!(?error, %project_id, "failed to get swarm project");
            ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "failed to get swarm project")
        })?
        .ok_or_else(|| ErrorResponse::new(StatusCode::NOT_FOUND, "swarm project not found"))?;

    ensure_member_access(state.pool(), existing.organization_id, ctx.user.id).await?;

    let mut tx = state.pool().begin().await.map_err(|error| {
        tracing::error!(?error, "failed to start transaction");
        ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "internal server error")
    })?;

    SwarmProjectRepository::delete(&mut tx, project_id)
        .await
        .map_err(|error| match error {
            SwarmProjectError::NotFound => {
                ErrorResponse::new(StatusCode::NOT_FOUND, "swarm project not found")
            }
            _ => {
                tracing::error!(?error, "failed to delete swarm project");
                ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "failed to delete swarm project")
            }
        })?;

    tx.commit().await.map_err(|error| {
        tracing::error!(?error, "failed to commit transaction");
        ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "internal server error")
    })?;

    Ok(StatusCode::NO_CONTENT)
}

#[instrument(
    name = "swarm_projects.merge",
    skip(state, ctx, payload),
    fields(target_id = %project_id, source_id = %payload.source_id, user_id = %ctx.user.id)
)]
async fn merge_swarm_projects(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
    Path(project_id): Path<Uuid>,
    Json(payload): Json<MergeSwarmProjectsRequest>,
) -> Result<Json<SwarmProjectResponse>, ErrorResponse> {
    // Get both projects to verify org access
    let target = SwarmProjectRepository::find_by_id(state.pool(), project_id)
        .await
        .map_err(|error| {
            tracing::error!(?error, %project_id, "failed to get target project");
            ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "failed to get swarm project")
        })?
        .ok_or_else(|| ErrorResponse::new(StatusCode::NOT_FOUND, "target swarm project not found"))?;

    let source = SwarmProjectRepository::find_by_id(state.pool(), payload.source_id)
        .await
        .map_err(|error| {
            tracing::error!(?error, source_id = %payload.source_id, "failed to get source project");
            ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "failed to get swarm project")
        })?
        .ok_or_else(|| ErrorResponse::new(StatusCode::NOT_FOUND, "source swarm project not found"))?;

    // Both must be in the same organization
    if target.organization_id != source.organization_id {
        return Err(ErrorResponse::new(
            StatusCode::BAD_REQUEST,
            "cannot merge projects from different organizations",
        ));
    }

    ensure_member_access(state.pool(), target.organization_id, ctx.user.id).await?;

    let mut tx = state.pool().begin().await.map_err(|error| {
        tracing::error!(?error, "failed to start transaction");
        ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "internal server error")
    })?;

    let project = SwarmProjectRepository::merge(&mut tx, payload.source_id, project_id)
        .await
        .map_err(|error| match error {
            SwarmProjectError::NotFound => {
                ErrorResponse::new(StatusCode::NOT_FOUND, "swarm project not found")
            }
            SwarmProjectError::CannotMergeSelf => {
                ErrorResponse::new(StatusCode::BAD_REQUEST, "cannot merge project with itself")
            }
            _ => {
                tracing::error!(?error, "failed to merge swarm projects");
                ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "failed to merge swarm projects")
            }
        })?;

    tx.commit().await.map_err(|error| {
        tracing::error!(?error, "failed to commit transaction");
        ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "internal server error")
    })?;

    Ok(Json(SwarmProjectResponse { project }))
}

// =====================
// Node Link Handlers
// =====================

#[instrument(
    name = "swarm_projects.list_nodes",
    skip(state, ctx),
    fields(project_id = %project_id, user_id = %ctx.user.id)
)]
async fn list_swarm_project_nodes(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
    Path(project_id): Path<Uuid>,
) -> Result<Json<ListSwarmProjectNodesResponse>, ErrorResponse> {
    // Get the project to verify org access
    let project = SwarmProjectRepository::find_by_id(state.pool(), project_id)
        .await
        .map_err(|error| {
            tracing::error!(?error, %project_id, "failed to get swarm project");
            ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "failed to get swarm project")
        })?
        .ok_or_else(|| ErrorResponse::new(StatusCode::NOT_FOUND, "swarm project not found"))?;

    ensure_member_access(state.pool(), project.organization_id, ctx.user.id).await?;

    let nodes = SwarmProjectRepository::list_nodes(state.pool(), project_id)
        .await
        .map_err(|error| {
            tracing::error!(?error, "failed to list swarm project nodes");
            ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "failed to list nodes")
        })?;

    Ok(Json(ListSwarmProjectNodesResponse { nodes }))
}

#[instrument(
    name = "swarm_projects.link_node",
    skip(state, ctx, payload),
    fields(project_id = %project_id, node_id = %payload.node_id, user_id = %ctx.user.id)
)]
async fn link_node(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
    Path(project_id): Path<Uuid>,
    Json(payload): Json<LinkNodeRequest>,
) -> Result<Json<SwarmProjectNodeResponse>, ErrorResponse> {
    // Get the project to verify org access
    let project = SwarmProjectRepository::find_by_id(state.pool(), project_id)
        .await
        .map_err(|error| {
            tracing::error!(?error, %project_id, "failed to get swarm project");
            ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "failed to get swarm project")
        })?
        .ok_or_else(|| ErrorResponse::new(StatusCode::NOT_FOUND, "swarm project not found"))?;

    ensure_member_access(state.pool(), project.organization_id, ctx.user.id).await?;

    let mut tx = state.pool().begin().await.map_err(|error| {
        tracing::error!(?error, "failed to start transaction");
        ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "internal server error")
    })?;

    let link = SwarmProjectRepository::link_node(
        &mut tx,
        LinkSwarmProjectNodeData {
            swarm_project_id: project_id,
            node_id: payload.node_id,
            local_project_id: payload.local_project_id,
            git_repo_path: payload.git_repo_path.clone(),
            os_type: payload.os_type,
        },
    )
    .await
    .map_err(|error| match error {
        SwarmProjectError::LinkAlreadyExists => {
            ErrorResponse::new(StatusCode::CONFLICT, "node already linked to this swarm project")
        }
        _ => {
            tracing::error!(?error, "failed to link node to swarm project");
            ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "failed to link node")
        }
    })?;

    // Also update node_local_projects.swarm_project_id so the Node Projects UI shows correct linked status
    if let Err(error) = NodeLocalProjectRepository::link_to_swarm(
        &mut tx,
        payload.node_id,
        payload.local_project_id,
        project_id,
    )
    .await
    {
        // Log but don't fail - the swarm_project_nodes link is the primary source of truth
        tracing::warn!(
            ?error,
            node_id = %payload.node_id,
            local_project_id = %payload.local_project_id,
            swarm_project_id = %project_id,
            "failed to update node_local_projects.swarm_project_id (non-fatal)"
        );
    }

    tx.commit().await.map_err(|error| {
        tracing::error!(?error, "failed to commit transaction");
        ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "internal server error")
    })?;

    Ok(Json(SwarmProjectNodeResponse { link }))
}

#[instrument(
    name = "swarm_projects.unlink_node",
    skip(state, ctx),
    fields(project_id = %project_id, node_id = %node_id, user_id = %ctx.user.id)
)]
async fn unlink_node(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
    Path((project_id, node_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, ErrorResponse> {
    // Get the project to verify org access
    let project = SwarmProjectRepository::find_by_id(state.pool(), project_id)
        .await
        .map_err(|error| {
            tracing::error!(?error, %project_id, "failed to get swarm project");
            ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "failed to get swarm project")
        })?
        .ok_or_else(|| ErrorResponse::new(StatusCode::NOT_FOUND, "swarm project not found"))?;

    ensure_member_access(state.pool(), project.organization_id, ctx.user.id).await?;

    // First, find the link to get the local_project_id (needed for updating node_local_projects)
    let link = SwarmProjectRepository::find_node_link(state.pool(), project_id, node_id)
        .await
        .map_err(|error| {
            tracing::error!(?error, "failed to find node link");
            ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "internal server error")
        })?
        .ok_or_else(|| ErrorResponse::new(StatusCode::NOT_FOUND, "node link not found"))?;

    let mut tx = state.pool().begin().await.map_err(|error| {
        tracing::error!(?error, "failed to start transaction");
        ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "internal server error")
    })?;

    SwarmProjectRepository::unlink_node(&mut tx, project_id, node_id)
        .await
        .map_err(|error| match error {
            SwarmProjectError::LinkNotFound => {
                ErrorResponse::new(StatusCode::NOT_FOUND, "node link not found")
            }
            _ => {
                tracing::error!(?error, "failed to unlink node from swarm project");
                ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "failed to unlink node")
            }
        })?;

    // Also clear node_local_projects.swarm_project_id so the Node Projects UI shows correct unlinked status
    if let Err(error) =
        NodeLocalProjectRepository::unlink_from_swarm(&mut tx, node_id, link.local_project_id).await
    {
        // Log but don't fail - the swarm_project_nodes unlink is the primary operation
        tracing::warn!(
            ?error,
            node_id = %node_id,
            local_project_id = %link.local_project_id,
            "failed to clear node_local_projects.swarm_project_id (non-fatal)"
        );
    }

    tx.commit().await.map_err(|error| {
        tracing::error!(?error, "failed to commit transaction");
        ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "internal server error")
    })?;

    Ok(StatusCode::NO_CONTENT)
}
