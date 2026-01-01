//! Routes for managing swarm labels in the Hive.
//!
//! Swarm labels are organization-global labels (project_id = NULL) that can be
//! used across all nodes in the swarm. This module provides endpoints for:
//! - Listing swarm labels for an organization
//! - Creating swarm labels
//! - Merging labels (combining two labels into one)
//! - Converting local labels to swarm labels and vice versa

use axum::{
    Json, Router,
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use tracing::instrument;
use uuid::Uuid;

use super::{error::ErrorResponse, organization_members::ensure_member_access};
use crate::{
    AppState,
    auth::RequestContext,
    db::labels::{CreateLabelData, Label, LabelError, LabelRepository, UpdateLabelData},
};

// =====================
// Query & Request Types
// =====================

#[derive(Debug, Deserialize)]
pub struct ListSwarmLabelsQuery {
    pub organization_id: Uuid,
}

#[derive(Debug, Deserialize)]
pub struct CreateSwarmLabelRequest {
    pub organization_id: Uuid,
    pub name: String,
    #[serde(default = "default_icon")]
    pub icon: String,
    #[serde(default = "default_color")]
    pub color: String,
}

fn default_icon() -> String {
    "tag".to_string()
}

fn default_color() -> String {
    "#6b7280".to_string()
}

#[derive(Debug, Deserialize)]
pub struct UpdateSwarmLabelRequest {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub version: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct MergeSwarmLabelsRequest {
    /// The label to merge into this one (will be deleted)
    pub source_id: Uuid,
}

#[derive(Debug, Deserialize)]
pub struct PromoteToSwarmRequest {
    /// The project-scoped label to promote to a swarm (org-global) label
    pub label_id: Uuid,
}

// =====================
// Response Types
// =====================

#[derive(Debug, Serialize, Deserialize)]
pub struct SwarmLabelResponse {
    pub label: Label,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ListSwarmLabelsResponse {
    pub labels: Vec<Label>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MergeLabelsResult {
    pub label: Label,
    /// Number of task-label associations that were migrated
    pub migrated_task_count: u64,
}

// =====================
// Router
// =====================

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/swarm/labels", get(list_swarm_labels).post(create_swarm_label))
        .route(
            "/swarm/labels/{label_id}",
            get(get_swarm_label)
                .patch(update_swarm_label)
                .delete(delete_swarm_label),
        )
        .route("/swarm/labels/{label_id}/merge", post(merge_swarm_labels))
        .route("/swarm/labels/promote", post(promote_to_swarm))
}

// =====================
// Handlers
// =====================

#[instrument(
    name = "swarm_labels.list",
    skip(state, ctx, params),
    fields(org_id = %params.organization_id, user_id = %ctx.user.id)
)]
async fn list_swarm_labels(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
    Query(params): Query<ListSwarmLabelsQuery>,
) -> Result<Json<ListSwarmLabelsResponse>, ErrorResponse> {
    ensure_member_access(state.pool(), params.organization_id, ctx.user.id).await?;

    let repo = LabelRepository::new(state.pool());

    // Swarm labels are org-global labels (project_id = NULL)
    let labels = repo
        .find_swarm_labels(params.organization_id)
        .await
        .map_err(|error| {
            tracing::error!(?error, "failed to list swarm labels");
            ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "failed to list swarm labels")
        })?;

    Ok(Json(ListSwarmLabelsResponse { labels }))
}

#[instrument(
    name = "swarm_labels.get",
    skip(state, ctx),
    fields(label_id = %label_id, user_id = %ctx.user.id)
)]
async fn get_swarm_label(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
    Path(label_id): Path<Uuid>,
) -> Result<Json<SwarmLabelResponse>, ErrorResponse> {
    let repo = LabelRepository::new(state.pool());

    let label = repo
        .find_by_id(label_id)
        .await
        .map_err(|error| {
            tracing::error!(?error, %label_id, "failed to get swarm label");
            ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "failed to get swarm label")
        })?
        .ok_or_else(|| ErrorResponse::new(StatusCode::NOT_FOUND, "swarm label not found"))?;

    // Verify it's actually a swarm label (project_id = NULL)
    if label.project_id.is_some() {
        return Err(ErrorResponse::new(
            StatusCode::BAD_REQUEST,
            "label is project-scoped, not a swarm label",
        ));
    }

    ensure_member_access(state.pool(), label.organization_id, ctx.user.id).await?;

    Ok(Json(SwarmLabelResponse { label }))
}

#[instrument(
    name = "swarm_labels.create",
    skip(state, ctx, payload),
    fields(user_id = %ctx.user.id, org_id = %payload.organization_id)
)]
async fn create_swarm_label(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
    Json(payload): Json<CreateSwarmLabelRequest>,
) -> Result<Json<SwarmLabelResponse>, ErrorResponse> {
    ensure_member_access(state.pool(), payload.organization_id, ctx.user.id).await?;

    let repo = LabelRepository::new(state.pool());

    // Create as a swarm label (project_id = NULL)
    let label = repo
        .create(CreateLabelData {
            organization_id: payload.organization_id,
            project_id: None, // Swarm labels are org-global
            origin_node_id: None,
            name: payload.name,
            icon: payload.icon,
            color: payload.color,
        })
        .await
        .map_err(|error| match error {
            LabelError::Conflict(msg) => ErrorResponse::new(StatusCode::CONFLICT, &msg),
            _ => {
                tracing::error!(?error, "failed to create swarm label");
                ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "failed to create swarm label")
            }
        })?;

    Ok(Json(SwarmLabelResponse { label }))
}

#[instrument(
    name = "swarm_labels.update",
    skip(state, ctx, payload),
    fields(label_id = %label_id, user_id = %ctx.user.id)
)]
async fn update_swarm_label(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
    Path(label_id): Path<Uuid>,
    Json(payload): Json<UpdateSwarmLabelRequest>,
) -> Result<Json<SwarmLabelResponse>, ErrorResponse> {
    let repo = LabelRepository::new(state.pool());

    // Get the label to verify access and that it's a swarm label
    let existing = repo
        .find_by_id(label_id)
        .await
        .map_err(|error| {
            tracing::error!(?error, %label_id, "failed to get swarm label");
            ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "failed to get swarm label")
        })?
        .ok_or_else(|| ErrorResponse::new(StatusCode::NOT_FOUND, "swarm label not found"))?;

    if existing.project_id.is_some() {
        return Err(ErrorResponse::new(
            StatusCode::BAD_REQUEST,
            "label is project-scoped, not a swarm label",
        ));
    }

    ensure_member_access(state.pool(), existing.organization_id, ctx.user.id).await?;

    let label = repo
        .update(
            label_id,
            UpdateLabelData {
                name: payload.name,
                icon: payload.icon,
                color: payload.color,
                version: payload.version,
            },
        )
        .await
        .map_err(|error| match error {
            LabelError::NotFound => ErrorResponse::new(StatusCode::NOT_FOUND, "swarm label not found"),
            LabelError::VersionMismatch => {
                ErrorResponse::new(StatusCode::CONFLICT, "label version mismatch")
            }
            LabelError::Conflict(msg) => ErrorResponse::new(StatusCode::CONFLICT, &msg),
            _ => {
                tracing::error!(?error, "failed to update swarm label");
                ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "failed to update swarm label")
            }
        })?;

    Ok(Json(SwarmLabelResponse { label }))
}

#[instrument(
    name = "swarm_labels.delete",
    skip(state, ctx),
    fields(label_id = %label_id, user_id = %ctx.user.id)
)]
async fn delete_swarm_label(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
    Path(label_id): Path<Uuid>,
) -> Result<StatusCode, ErrorResponse> {
    let repo = LabelRepository::new(state.pool());

    // Get the label to verify access and that it's a swarm label
    let existing = repo
        .find_by_id(label_id)
        .await
        .map_err(|error| {
            tracing::error!(?error, %label_id, "failed to get swarm label");
            ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "failed to get swarm label")
        })?
        .ok_or_else(|| ErrorResponse::new(StatusCode::NOT_FOUND, "swarm label not found"))?;

    if existing.project_id.is_some() {
        return Err(ErrorResponse::new(
            StatusCode::BAD_REQUEST,
            "label is project-scoped, not a swarm label",
        ));
    }

    ensure_member_access(state.pool(), existing.organization_id, ctx.user.id).await?;

    repo.delete(label_id, None)
        .await
        .map_err(|error| match error {
            LabelError::NotFound => ErrorResponse::new(StatusCode::NOT_FOUND, "swarm label not found"),
            _ => {
                tracing::error!(?error, "failed to delete swarm label");
                ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "failed to delete swarm label")
            }
        })?;

    Ok(StatusCode::NO_CONTENT)
}

#[instrument(
    name = "swarm_labels.merge",
    skip(state, ctx, payload),
    fields(target_id = %label_id, source_id = %payload.source_id, user_id = %ctx.user.id)
)]
async fn merge_swarm_labels(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
    Path(label_id): Path<Uuid>,
    Json(payload): Json<MergeSwarmLabelsRequest>,
) -> Result<Json<MergeLabelsResult>, ErrorResponse> {
    let repo = LabelRepository::new(state.pool());

    // Get both labels to verify access and that they're swarm labels
    let target = repo
        .find_by_id(label_id)
        .await
        .map_err(|error| {
            tracing::error!(?error, %label_id, "failed to get target label");
            ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "failed to get swarm label")
        })?
        .ok_or_else(|| ErrorResponse::new(StatusCode::NOT_FOUND, "target swarm label not found"))?;

    let source = repo
        .find_by_id(payload.source_id)
        .await
        .map_err(|error| {
            tracing::error!(?error, source_id = %payload.source_id, "failed to get source label");
            ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "failed to get swarm label")
        })?
        .ok_or_else(|| ErrorResponse::new(StatusCode::NOT_FOUND, "source swarm label not found"))?;

    // Both must be swarm labels (project_id = NULL)
    if target.project_id.is_some() {
        return Err(ErrorResponse::new(
            StatusCode::BAD_REQUEST,
            "target label is project-scoped, not a swarm label",
        ));
    }
    if source.project_id.is_some() {
        return Err(ErrorResponse::new(
            StatusCode::BAD_REQUEST,
            "source label is project-scoped, not a swarm label",
        ));
    }

    // Both must be in the same organization
    if target.organization_id != source.organization_id {
        return Err(ErrorResponse::new(
            StatusCode::BAD_REQUEST,
            "cannot merge labels from different organizations",
        ));
    }

    // Cannot merge with self
    if label_id == payload.source_id {
        return Err(ErrorResponse::new(
            StatusCode::BAD_REQUEST,
            "cannot merge label with itself",
        ));
    }

    ensure_member_access(state.pool(), target.organization_id, ctx.user.id).await?;

    // Perform the merge
    let (label, migrated_count) = repo
        .merge_labels(payload.source_id, label_id)
        .await
        .map_err(|error| {
            tracing::error!(?error, "failed to merge swarm labels");
            ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "failed to merge swarm labels")
        })?;

    Ok(Json(MergeLabelsResult {
        label,
        migrated_task_count: migrated_count,
    }))
}

#[instrument(
    name = "swarm_labels.promote",
    skip(state, ctx, payload),
    fields(label_id = %payload.label_id, user_id = %ctx.user.id)
)]
async fn promote_to_swarm(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
    Json(payload): Json<PromoteToSwarmRequest>,
) -> Result<Json<SwarmLabelResponse>, ErrorResponse> {
    let repo = LabelRepository::new(state.pool());

    // Get the label to verify it exists and is project-scoped
    let existing = repo
        .find_by_id(payload.label_id)
        .await
        .map_err(|error| {
            tracing::error!(?error, label_id = %payload.label_id, "failed to get label");
            ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "failed to get label")
        })?
        .ok_or_else(|| ErrorResponse::new(StatusCode::NOT_FOUND, "label not found"))?;

    if existing.project_id.is_none() {
        return Err(ErrorResponse::new(
            StatusCode::BAD_REQUEST,
            "label is already a swarm label",
        ));
    }

    ensure_member_access(state.pool(), existing.organization_id, ctx.user.id).await?;

    // Promote to swarm label by setting project_id to NULL
    let label = repo
        .promote_to_swarm(payload.label_id)
        .await
        .map_err(|error| match error {
            LabelError::NotFound => ErrorResponse::new(StatusCode::NOT_FOUND, "label not found"),
            LabelError::Conflict(msg) => ErrorResponse::new(StatusCode::CONFLICT, &msg),
            _ => {
                tracing::error!(?error, "failed to promote label to swarm");
                ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "failed to promote label to swarm")
            }
        })?;

    Ok(Json(SwarmLabelResponse { label }))
}
