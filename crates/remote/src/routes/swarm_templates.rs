//! Routes for managing swarm templates in the Hive.
//!
//! Swarm templates are organization-wide templates that can be used across
//! all nodes in the swarm. This module provides endpoints for:
//! - Listing swarm templates for an organization
//! - Creating swarm templates
//! - Updating and deleting swarm templates
//! - Merging templates (consolidating duplicates)

use axum::{
    Json, Router,
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::instrument;
use uuid::Uuid;

use super::{error::ErrorResponse, organization_members::ensure_member_access};
use crate::{
    AppState,
    auth::RequestContext,
    db::swarm_templates::{
        CreateSwarmTemplateData, SwarmTemplate, SwarmTemplateError, SwarmTemplateRepository,
        UpdateSwarmTemplateData,
    },
};

// =====================
// Query & Request Types
// =====================

#[derive(Debug, Deserialize)]
pub struct ListSwarmTemplatesQuery {
    pub organization_id: Uuid,
}

#[derive(Debug, Deserialize)]
pub struct CreateSwarmTemplateRequest {
    pub organization_id: Uuid,
    pub name: String,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Debug, Deserialize)]
pub struct UpdateSwarmTemplateRequest {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub metadata: Option<Value>,
    #[serde(default)]
    pub version: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct MergeSwarmTemplatesRequest {
    /// The template to merge into this one (will be soft-deleted)
    pub source_id: Uuid,
}

// =====================
// Response Types
// =====================

#[derive(Debug, Serialize, Deserialize)]
pub struct SwarmTemplateResponse {
    pub template: SwarmTemplate,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ListSwarmTemplatesResponse {
    pub templates: Vec<SwarmTemplate>,
}

// =====================
// Router
// =====================

pub fn router() -> Router<AppState> {
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

// =====================
// Handlers
// =====================

#[instrument(
    name = "swarm_templates.list",
    skip(state, ctx, params),
    fields(org_id = %params.organization_id, user_id = %ctx.user.id)
)]
async fn list_swarm_templates(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
    Query(params): Query<ListSwarmTemplatesQuery>,
) -> Result<Json<ListSwarmTemplatesResponse>, ErrorResponse> {
    ensure_member_access(state.pool(), params.organization_id, ctx.user.id).await?;

    let templates =
        SwarmTemplateRepository::list_by_organization(state.pool(), params.organization_id)
            .await
            .map_err(|error| {
                tracing::error!(?error, "failed to list swarm templates");
                ErrorResponse::new(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "failed to list swarm templates",
                )
            })?;

    Ok(Json(ListSwarmTemplatesResponse { templates }))
}

#[instrument(
    name = "swarm_templates.get",
    skip(state, ctx),
    fields(template_id = %template_id, user_id = %ctx.user.id)
)]
async fn get_swarm_template(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
    Path(template_id): Path<Uuid>,
) -> Result<Json<SwarmTemplateResponse>, ErrorResponse> {
    let template = SwarmTemplateRepository::find_by_id(state.pool(), template_id)
        .await
        .map_err(|error| {
            tracing::error!(?error, %template_id, "failed to get swarm template");
            ErrorResponse::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "failed to get swarm template",
            )
        })?
        .ok_or_else(|| ErrorResponse::new(StatusCode::NOT_FOUND, "swarm template not found"))?;

    ensure_member_access(state.pool(), template.organization_id, ctx.user.id).await?;

    Ok(Json(SwarmTemplateResponse { template }))
}

#[instrument(
    name = "swarm_templates.create",
    skip(state, ctx, payload),
    fields(user_id = %ctx.user.id, org_id = %payload.organization_id)
)]
async fn create_swarm_template(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
    Json(payload): Json<CreateSwarmTemplateRequest>,
) -> Result<Json<SwarmTemplateResponse>, ErrorResponse> {
    ensure_member_access(state.pool(), payload.organization_id, ctx.user.id).await?;

    let mut tx = state.pool().begin().await.map_err(|error| {
        tracing::error!(?error, "failed to start transaction");
        ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "internal server error")
    })?;

    let template = SwarmTemplateRepository::create(
        &mut tx,
        CreateSwarmTemplateData {
            organization_id: payload.organization_id,
            name: payload.name,
            content: payload.content,
            description: payload.description,
            metadata: payload.metadata,
        },
    )
    .await
    .map_err(|error| match error {
        SwarmTemplateError::NameConflict => {
            ErrorResponse::new(StatusCode::CONFLICT, "swarm template name already exists")
        }
        SwarmTemplateError::InvalidMetadata => {
            ErrorResponse::new(StatusCode::BAD_REQUEST, "metadata must be a JSON object")
        }
        _ => {
            tracing::error!(?error, "failed to create swarm template");
            ErrorResponse::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "failed to create swarm template",
            )
        }
    })?;

    tx.commit().await.map_err(|error| {
        tracing::error!(?error, "failed to commit transaction");
        ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "internal server error")
    })?;

    Ok(Json(SwarmTemplateResponse { template }))
}

#[instrument(
    name = "swarm_templates.update",
    skip(state, ctx, payload),
    fields(template_id = %template_id, user_id = %ctx.user.id)
)]
async fn update_swarm_template(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
    Path(template_id): Path<Uuid>,
    Json(payload): Json<UpdateSwarmTemplateRequest>,
) -> Result<Json<SwarmTemplateResponse>, ErrorResponse> {
    // Get the template to verify org access
    let existing = SwarmTemplateRepository::find_by_id(state.pool(), template_id)
        .await
        .map_err(|error| {
            tracing::error!(?error, %template_id, "failed to get swarm template");
            ErrorResponse::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "failed to get swarm template",
            )
        })?
        .ok_or_else(|| ErrorResponse::new(StatusCode::NOT_FOUND, "swarm template not found"))?;

    ensure_member_access(state.pool(), existing.organization_id, ctx.user.id).await?;

    let mut tx = state.pool().begin().await.map_err(|error| {
        tracing::error!(?error, "failed to start transaction");
        ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "internal server error")
    })?;

    let template = SwarmTemplateRepository::update(
        &mut tx,
        template_id,
        UpdateSwarmTemplateData {
            name: payload.name,
            content: payload.content,
            description: payload.description,
            metadata: payload.metadata,
            version: payload.version,
        },
    )
    .await
    .map_err(|error| match error {
        SwarmTemplateError::NotFound => {
            ErrorResponse::new(StatusCode::NOT_FOUND, "swarm template not found")
        }
        SwarmTemplateError::VersionMismatch => {
            ErrorResponse::new(StatusCode::CONFLICT, "template version mismatch")
        }
        SwarmTemplateError::NameConflict => {
            ErrorResponse::new(StatusCode::CONFLICT, "swarm template name already exists")
        }
        SwarmTemplateError::InvalidMetadata => {
            ErrorResponse::new(StatusCode::BAD_REQUEST, "metadata must be a JSON object")
        }
        _ => {
            tracing::error!(?error, "failed to update swarm template");
            ErrorResponse::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "failed to update swarm template",
            )
        }
    })?;

    tx.commit().await.map_err(|error| {
        tracing::error!(?error, "failed to commit transaction");
        ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "internal server error")
    })?;

    Ok(Json(SwarmTemplateResponse { template }))
}

#[instrument(
    name = "swarm_templates.delete",
    skip(state, ctx),
    fields(template_id = %template_id, user_id = %ctx.user.id)
)]
async fn delete_swarm_template(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
    Path(template_id): Path<Uuid>,
) -> Result<StatusCode, ErrorResponse> {
    // Get the template to verify org access
    let existing = SwarmTemplateRepository::find_by_id(state.pool(), template_id)
        .await
        .map_err(|error| {
            tracing::error!(?error, %template_id, "failed to get swarm template");
            ErrorResponse::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "failed to get swarm template",
            )
        })?
        .ok_or_else(|| ErrorResponse::new(StatusCode::NOT_FOUND, "swarm template not found"))?;

    ensure_member_access(state.pool(), existing.organization_id, ctx.user.id).await?;

    let mut tx = state.pool().begin().await.map_err(|error| {
        tracing::error!(?error, "failed to start transaction");
        ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "internal server error")
    })?;

    SwarmTemplateRepository::delete(&mut tx, template_id, None)
        .await
        .map_err(|error| match error {
            SwarmTemplateError::NotFound => {
                ErrorResponse::new(StatusCode::NOT_FOUND, "swarm template not found")
            }
            _ => {
                tracing::error!(?error, "failed to delete swarm template");
                ErrorResponse::new(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "failed to delete swarm template",
                )
            }
        })?;

    tx.commit().await.map_err(|error| {
        tracing::error!(?error, "failed to commit transaction");
        ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "internal server error")
    })?;

    Ok(StatusCode::NO_CONTENT)
}

#[instrument(
    name = "swarm_templates.merge",
    skip(state, ctx, payload),
    fields(target_id = %template_id, source_id = %payload.source_id, user_id = %ctx.user.id)
)]
async fn merge_swarm_templates(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
    Path(template_id): Path<Uuid>,
    Json(payload): Json<MergeSwarmTemplatesRequest>,
) -> Result<Json<SwarmTemplateResponse>, ErrorResponse> {
    // Get both templates to verify org access
    let target = SwarmTemplateRepository::find_by_id(state.pool(), template_id)
        .await
        .map_err(|error| {
            tracing::error!(?error, %template_id, "failed to get target template");
            ErrorResponse::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "failed to get swarm template",
            )
        })?
        .ok_or_else(|| {
            ErrorResponse::new(StatusCode::NOT_FOUND, "target swarm template not found")
        })?;

    let source = SwarmTemplateRepository::find_by_id(state.pool(), payload.source_id)
        .await
        .map_err(|error| {
            tracing::error!(?error, source_id = %payload.source_id, "failed to get source template");
            ErrorResponse::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "failed to get swarm template",
            )
        })?
        .ok_or_else(|| {
            ErrorResponse::new(StatusCode::NOT_FOUND, "source swarm template not found")
        })?;

    // Both must be in the same organization
    if target.organization_id != source.organization_id {
        return Err(ErrorResponse::new(
            StatusCode::BAD_REQUEST,
            "cannot merge templates from different organizations",
        ));
    }

    // Cannot merge with self
    if template_id == payload.source_id {
        return Err(ErrorResponse::new(
            StatusCode::BAD_REQUEST,
            "cannot merge template with itself",
        ));
    }

    ensure_member_access(state.pool(), target.organization_id, ctx.user.id).await?;

    let mut tx = state.pool().begin().await.map_err(|error| {
        tracing::error!(?error, "failed to start transaction");
        ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "internal server error")
    })?;

    let template = SwarmTemplateRepository::merge(&mut tx, payload.source_id, template_id)
        .await
        .map_err(|error| match error {
            SwarmTemplateError::NotFound => {
                ErrorResponse::new(StatusCode::NOT_FOUND, "swarm template not found")
            }
            SwarmTemplateError::CannotMergeSelf => {
                ErrorResponse::new(StatusCode::BAD_REQUEST, "cannot merge template with itself")
            }
            _ => {
                tracing::error!(?error, "failed to merge swarm templates");
                ErrorResponse::new(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "failed to merge swarm templates",
                )
            }
        })?;

    tx.commit().await.map_err(|error| {
        tracing::error!(?error, "failed to commit transaction");
        ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "internal server error")
    })?;

    Ok(Json(SwarmTemplateResponse { template }))
}
