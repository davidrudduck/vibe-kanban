use axum::{
    Extension, Json, Router,
    extract::{Query, State},
    middleware::from_fn_with_state,
    response::Json as ResponseJson,
    routing::{get, put},
};
use db::models::template::{CreateTemplate, Template, UpdateTemplate};
use deployment::Deployment;
use serde::Deserialize;
use ts_rs::TS;
use utils::response::ApiResponse;

use crate::{DeploymentImpl, error::ApiError, middleware::load_template_middleware};

#[derive(Deserialize, TS)]
pub struct TemplateSearchParams {
    #[serde(default)]
    pub search: Option<String>,
}

pub async fn get_templates(
    State(deployment): State<DeploymentImpl>,
    Query(params): Query<TemplateSearchParams>,
) -> Result<ResponseJson<ApiResponse<Vec<Template>>>, ApiError> {
    let mut templates = Template::find_all(&deployment.db().pool).await?;

    // Filter by search query if provided
    if let Some(search_query) = params.search {
        let search_lower = search_query.to_lowercase();
        templates.retain(|template| {
            template
                .template_name
                .to_lowercase()
                .contains(&search_lower)
        });
    }

    Ok(ResponseJson(ApiResponse::success(templates)))
}

pub async fn create_template(
    State(deployment): State<DeploymentImpl>,
    Json(payload): Json<CreateTemplate>,
) -> Result<ResponseJson<ApiResponse<Template>>, ApiError> {
    let template = Template::create(&deployment.db().pool, &payload).await?;

    Ok(ResponseJson(ApiResponse::success(template)))
}

pub async fn update_template(
    Extension(template): Extension<Template>,
    State(deployment): State<DeploymentImpl>,
    Json(payload): Json<UpdateTemplate>,
) -> Result<ResponseJson<ApiResponse<Template>>, ApiError> {
    let updated_template = Template::update(&deployment.db().pool, template.id, &payload).await?;

    Ok(ResponseJson(ApiResponse::success(updated_template)))
}

pub async fn delete_template(
    Extension(template): Extension<Template>,
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<()>>, ApiError> {
    let rows_affected = Template::delete(&deployment.db().pool, template.id).await?;
    if rows_affected == 0 {
        Err(ApiError::Database(sqlx::Error::RowNotFound))
    } else {
        Ok(ResponseJson(ApiResponse::success(())))
    }
}

pub fn router(deployment: &DeploymentImpl) -> Router<DeploymentImpl> {
    let template_router = Router::new()
        .route("/", put(update_template).delete(delete_template))
        .layer(from_fn_with_state(
            deployment.clone(),
            load_template_middleware,
        ));

    let inner = Router::new()
        .route("/", get(get_templates).post(create_template))
        .nest("/{template_id}", template_router);

    Router::new().nest("/templates", inner)
}
