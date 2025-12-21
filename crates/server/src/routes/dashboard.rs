use axum::{
    Router,
    extract::{Query, State},
    response::Json as ResponseJson,
    routing::get,
};
use db::models::activity_feed::ActivityFeed;
use db::models::dashboard::DashboardSummary;
use deployment::Deployment;
use serde::Deserialize;
use utils::response::ApiResponse;

use crate::{DeploymentImpl, error::ApiError};

/// Query parameters for the activity feed endpoint.
#[derive(Debug, Deserialize)]
pub struct ActivityFeedQuery {
    /// If true, includes dismissed items in the feed. Defaults to false.
    #[serde(default)]
    pub include_dismissed: bool,
}

/// Get dashboard summary of active tasks across all projects
pub async fn get_dashboard_summary(
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<DashboardSummary>>, ApiError> {
    let summary = DashboardSummary::fetch(&deployment.db().pool).await?;
    Ok(ResponseJson(ApiResponse::success(summary)))
}

/// Get activity feed for notification popover
pub async fn get_activity_feed(
    State(deployment): State<DeploymentImpl>,
    Query(query): Query<ActivityFeedQuery>,
) -> Result<ResponseJson<ApiResponse<ActivityFeed>>, ApiError> {
    let feed = ActivityFeed::fetch(&deployment.db().pool, query.include_dismissed).await?;
    Ok(ResponseJson(ApiResponse::success(feed)))
}

pub fn router(_deployment: &DeploymentImpl) -> Router<DeploymentImpl> {
    Router::new().nest(
        "/dashboard",
        Router::new()
            .route("/summary", get(get_dashboard_summary))
            .route("/activity", get(get_activity_feed)),
    )
}
