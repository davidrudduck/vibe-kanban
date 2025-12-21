use axum::{
    Router,
    extract::{Json, Path, Query, State},
    response::Json as ResponseJson,
    routing::{delete, get, post},
};
use db::models::activity_dismissal::ActivityDismissal;
use db::models::activity_feed::ActivityFeed;
use db::models::dashboard::DashboardSummary;
use deployment::Deployment;
use serde::Deserialize;
use utils::response::ApiResponse;
use uuid::Uuid;

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

/// Request body for dismissing an activity item.
#[derive(Debug, Deserialize)]
pub struct DismissActivityRequest {
    pub task_id: Uuid,
}

/// Dismiss an activity item (mark as reviewed).
pub async fn dismiss_activity_item(
    State(deployment): State<DeploymentImpl>,
    Json(payload): Json<DismissActivityRequest>,
) -> Result<ResponseJson<ApiResponse<ActivityDismissal>>, ApiError> {
    let dismissal = ActivityDismissal::dismiss(&deployment.db().pool, payload.task_id).await?;
    Ok(ResponseJson(ApiResponse::success(dismissal)))
}

/// Undismiss an activity item (remove from dismissed).
pub async fn undismiss_activity_item(
    State(deployment): State<DeploymentImpl>,
    Path(task_id): Path<Uuid>,
) -> Result<ResponseJson<ApiResponse<()>>, ApiError> {
    ActivityDismissal::undismiss(&deployment.db().pool, task_id).await?;
    Ok(ResponseJson(ApiResponse::success(())))
}

pub fn router(_deployment: &DeploymentImpl) -> Router<DeploymentImpl> {
    Router::new().nest(
        "/dashboard",
        Router::new()
            .route("/summary", get(get_dashboard_summary))
            .route("/activity", get(get_activity_feed))
            .route("/activity/dismiss", post(dismiss_activity_item))
            .route("/activity/dismiss/{task_id}", delete(undismiss_activity_item)),
    )
}
