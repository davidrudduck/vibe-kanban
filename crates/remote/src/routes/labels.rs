use axum::{
    Json, Router,
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{delete, get, patch, post},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::instrument;
use uuid::Uuid;

use super::organization_members::{
    ensure_member_access, ensure_project_access, ensure_task_access,
};
use crate::{
    AppState,
    auth::RequestContext,
    db::labels::{CreateLabelData, Label, LabelError, LabelRepository, UpdateLabelData},
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/labels", get(list_labels))
        .route("/labels", post(create_label))
        .route("/labels/{label_id}", get(get_label))
        .route("/labels/{label_id}", patch(update_label))
        .route("/labels/{label_id}", delete(delete_label))
        .route("/tasks/{task_id}/labels", get(get_task_labels))
        .route("/tasks/{task_id}/labels", post(set_task_labels))
        .route("/tasks/{task_id}/labels/{label_id}", post(attach_label))
        .route("/tasks/{task_id}/labels/{label_id}", delete(detach_label))
}

#[derive(Debug, Deserialize)]
pub struct ListLabelsQuery {
    pub organization_id: Uuid,
    pub project_id: Option<Uuid>,
}

#[derive(Debug, Serialize)]
pub struct ListLabelsResponse {
    pub labels: Vec<Label>,
}

#[instrument(
    name = "labels.list",
    skip(state, ctx, query),
    fields(user_id = %ctx.user.id, org_id = %query.organization_id)
)]
pub async fn list_labels(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
    Query(query): Query<ListLabelsQuery>,
) -> Response {
    let pool = state.pool();

    if let Err(error) = ensure_member_access(pool, query.organization_id, ctx.user.id).await {
        return error.into_response();
    }

    let repo = LabelRepository::new(pool);
    let result = match query.project_id {
        Some(project_id) => {
            repo.find_for_project(query.organization_id, project_id)
                .await
        }
        None => repo.find_by_organization(query.organization_id).await,
    };

    match result {
        Ok(labels) => (StatusCode::OK, Json(ListLabelsResponse { labels })).into_response(),
        Err(error) => label_error_response(error, "failed to list labels"),
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateLabelRequest {
    pub organization_id: Uuid,
    pub project_id: Option<Uuid>,
    pub origin_node_id: Option<Uuid>,
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

#[derive(Debug, Serialize)]
pub struct LabelResponse {
    pub label: Label,
}

#[instrument(
    name = "labels.create",
    skip(state, ctx, payload),
    fields(user_id = %ctx.user.id, org_id = %payload.organization_id)
)]
pub async fn create_label(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
    Json(payload): Json<CreateLabelRequest>,
) -> Response {
    let pool = state.pool();

    // Verify access to organization (and project if specified)
    if let Some(project_id) = payload.project_id {
        if let Err(error) = ensure_project_access(pool, ctx.user.id, project_id).await {
            return error.into_response();
        }
    } else if let Err(error) =
        ensure_member_access(pool, payload.organization_id, ctx.user.id).await
    {
        return error.into_response();
    }

    let repo = LabelRepository::new(pool);
    let data = CreateLabelData {
        organization_id: payload.organization_id,
        project_id: payload.project_id,
        origin_node_id: payload.origin_node_id,
        name: payload.name,
        icon: payload.icon,
        color: payload.color,
    };

    match repo.create(data).await {
        Ok(label) => (StatusCode::CREATED, Json(LabelResponse { label })).into_response(),
        Err(error) => label_error_response(error, "failed to create label"),
    }
}

#[instrument(
    name = "labels.get",
    skip(state, ctx),
    fields(user_id = %ctx.user.id, label_id = %label_id)
)]
pub async fn get_label(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
    Path(label_id): Path<Uuid>,
) -> Response {
    let pool = state.pool();
    let repo = LabelRepository::new(pool);

    let label = match repo.find_by_id(label_id).await {
        Ok(Some(l)) => l,
        Ok(None) => return label_error_response(LabelError::NotFound, "label not found"),
        Err(error) => return label_error_response(error, "failed to get label"),
    };

    // Verify user has access to the organization
    if let Err(error) = ensure_member_access(pool, label.organization_id, ctx.user.id).await {
        return error.into_response();
    }

    (StatusCode::OK, Json(LabelResponse { label })).into_response()
}

#[derive(Debug, Deserialize)]
pub struct UpdateLabelRequest {
    pub name: Option<String>,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub version: Option<i64>,
}

#[instrument(
    name = "labels.update",
    skip(state, ctx, payload),
    fields(user_id = %ctx.user.id, label_id = %label_id)
)]
pub async fn update_label(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
    Path(label_id): Path<Uuid>,
    Json(payload): Json<UpdateLabelRequest>,
) -> Response {
    let pool = state.pool();
    let repo = LabelRepository::new(pool);

    // First get the label to check organization access
    let existing = match repo.find_by_id(label_id).await {
        Ok(Some(l)) => l,
        Ok(None) => return label_error_response(LabelError::NotFound, "label not found"),
        Err(error) => return label_error_response(error, "failed to get label"),
    };

    if let Err(error) = ensure_member_access(pool, existing.organization_id, ctx.user.id).await {
        return error.into_response();
    }

    let data = UpdateLabelData {
        name: payload.name,
        icon: payload.icon,
        color: payload.color,
        version: payload.version,
    };

    match repo.update(label_id, data).await {
        Ok(label) => (StatusCode::OK, Json(LabelResponse { label })).into_response(),
        Err(error) => label_error_response(error, "failed to update label"),
    }
}

#[derive(Debug, Deserialize)]
pub struct DeleteLabelRequest {
    pub version: Option<i64>,
}

#[instrument(
    name = "labels.delete",
    skip(state, ctx, payload),
    fields(user_id = %ctx.user.id, label_id = %label_id)
)]
pub async fn delete_label(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
    Path(label_id): Path<Uuid>,
    payload: Option<Json<DeleteLabelRequest>>,
) -> Response {
    let pool = state.pool();
    let repo = LabelRepository::new(pool);

    // First get the label to check organization access
    let existing = match repo.find_by_id(label_id).await {
        Ok(Some(l)) => l,
        Ok(None) => return label_error_response(LabelError::NotFound, "label not found"),
        Err(error) => return label_error_response(error, "failed to get label"),
    };

    if let Err(error) = ensure_member_access(pool, existing.organization_id, ctx.user.id).await {
        return error.into_response();
    }

    let version = payload.as_ref().and_then(|p| p.version);

    match repo.delete(label_id, version).await {
        Ok(label) => (StatusCode::OK, Json(LabelResponse { label })).into_response(),
        Err(error) => label_error_response(error, "failed to delete label"),
    }
}

// Task label endpoints

#[derive(Debug, Serialize)]
pub struct TaskLabelsResponse {
    pub labels: Vec<Label>,
}

#[instrument(
    name = "labels.get_task_labels",
    skip(state, ctx),
    fields(user_id = %ctx.user.id, task_id = %task_id)
)]
pub async fn get_task_labels(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
    Path(task_id): Path<Uuid>,
) -> Response {
    let pool = state.pool();

    if let Err(error) = ensure_task_access(pool, ctx.user.id, task_id).await {
        return error.into_response();
    }

    let repo = LabelRepository::new(pool);
    match repo.find_by_task(task_id).await {
        Ok(labels) => (StatusCode::OK, Json(TaskLabelsResponse { labels })).into_response(),
        Err(error) => label_error_response(error, "failed to get task labels"),
    }
}

#[derive(Debug, Deserialize)]
pub struct SetTaskLabelsRequest {
    pub label_ids: Vec<Uuid>,
}

#[instrument(
    name = "labels.set_task_labels",
    skip(state, ctx, payload),
    fields(user_id = %ctx.user.id, task_id = %task_id)
)]
pub async fn set_task_labels(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
    Path(task_id): Path<Uuid>,
    Json(payload): Json<SetTaskLabelsRequest>,
) -> Response {
    let pool = state.pool();

    if let Err(error) = ensure_task_access(pool, ctx.user.id, task_id).await {
        return error.into_response();
    }

    let repo = LabelRepository::new(pool);
    match repo.set_task_labels(task_id, &payload.label_ids).await {
        Ok(labels) => (StatusCode::OK, Json(TaskLabelsResponse { labels })).into_response(),
        Err(error) => label_error_response(error, "failed to set task labels"),
    }
}

#[instrument(
    name = "labels.attach",
    skip(state, ctx),
    fields(user_id = %ctx.user.id, task_id = %task_id, label_id = %label_id)
)]
pub async fn attach_label(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
    Path((task_id, label_id)): Path<(Uuid, Uuid)>,
) -> Response {
    let pool = state.pool();

    if let Err(error) = ensure_task_access(pool, ctx.user.id, task_id).await {
        return error.into_response();
    }

    let repo = LabelRepository::new(pool);
    match repo.attach_to_task(task_id, label_id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => label_error_response(error, "failed to attach label"),
    }
}

#[instrument(
    name = "labels.detach",
    skip(state, ctx),
    fields(user_id = %ctx.user.id, task_id = %task_id, label_id = %label_id)
)]
pub async fn detach_label(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
    Path((task_id, label_id)): Path<(Uuid, Uuid)>,
) -> Response {
    let pool = state.pool();

    if let Err(error) = ensure_task_access(pool, ctx.user.id, task_id).await {
        return error.into_response();
    }

    let repo = LabelRepository::new(pool);
    match repo.detach_from_task(task_id, label_id).await {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => label_error_response(error, "failed to detach label"),
    }
}

// Error handling

fn label_error_response(error: LabelError, context: &str) -> Response {
    match error {
        LabelError::NotFound => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "label not found" })),
        ),
        LabelError::Conflict(message) => (StatusCode::CONFLICT, Json(json!({ "error": message }))),
        LabelError::VersionMismatch => (
            StatusCode::CONFLICT,
            Json(json!({ "error": "label version mismatch" })),
        ),
        LabelError::Database(err) => {
            tracing::error!(?err, "{context}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "internal server error" })),
            )
        }
    }
    .into_response()
}
