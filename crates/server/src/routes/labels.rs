use axum::{
    Extension, Json, Router,
    extract::{Query, State},
    middleware::from_fn_with_state,
    response::Json as ResponseJson,
    routing::get,
};
use db::models::label::{CreateLabel, Label, UpdateLabel};
use deployment::Deployment;
use serde::Deserialize;
use ts_rs::TS;
use utils::response::ApiResponse;
use uuid::Uuid;

use crate::{DeploymentImpl, error::ApiError, middleware::load_label_middleware};

#[derive(Deserialize, TS)]
pub struct LabelQueryParams {
    /// Filter by project ID. If provided, returns global + project-specific labels.
    /// If not provided, returns only global labels.
    #[serde(default)]
    pub project_id: Option<Uuid>,
}

/// GET /api/labels - List labels
/// Returns global labels if no project_id provided, otherwise global + project-specific
pub async fn get_labels(
    State(deployment): State<DeploymentImpl>,
    Query(params): Query<LabelQueryParams>,
) -> Result<ResponseJson<ApiResponse<Vec<Label>>>, ApiError> {
    let labels = Label::find_for_project(&deployment.db().pool, params.project_id).await?;
    Ok(ResponseJson(ApiResponse::success(labels)))
}

/// POST /api/labels - Create a new label
pub async fn create_label(
    State(deployment): State<DeploymentImpl>,
    Json(payload): Json<CreateLabel>,
) -> Result<ResponseJson<ApiResponse<Label>>, ApiError> {
    let label = Label::create(&deployment.db().pool, &payload).await?;
    Ok(ResponseJson(ApiResponse::success(label)))
}

/// GET /api/labels/{id} - Get a label by ID
pub async fn get_label(
    Extension(label): Extension<Label>,
) -> Result<ResponseJson<ApiResponse<Label>>, ApiError> {
    Ok(ResponseJson(ApiResponse::success(label)))
}

/// PUT /api/labels/{id} - Update a label
pub async fn update_label(
    Extension(label): Extension<Label>,
    State(deployment): State<DeploymentImpl>,
    Json(payload): Json<UpdateLabel>,
) -> Result<ResponseJson<ApiResponse<Label>>, ApiError> {
    let updated_label = Label::update(&deployment.db().pool, label.id, &payload).await?;
    Ok(ResponseJson(ApiResponse::success(updated_label)))
}

/// DELETE /api/labels/{id} - Delete a label
pub async fn delete_label(
    Extension(label): Extension<Label>,
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<()>>, ApiError> {
    let rows_affected = Label::delete(&deployment.db().pool, label.id).await?;
    if rows_affected == 0 {
        Err(ApiError::Database(sqlx::Error::RowNotFound))
    } else {
        Ok(ResponseJson(ApiResponse::success(())))
    }
}

pub fn router(deployment: &DeploymentImpl) -> Router<DeploymentImpl> {
    let label_router = Router::new()
        .route("/", get(get_label).put(update_label).delete(delete_label))
        .layer(from_fn_with_state(
            deployment.clone(),
            load_label_middleware,
        ));

    let inner = Router::new()
        .route("/", get(get_labels).post(create_label))
        .nest("/{label_id}", label_router);

    Router::new().nest("/labels", inner)
}
