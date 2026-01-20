use axum::{
    Json, Router,
    extract::{Extension, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use serde::Deserialize;
use tracing::instrument;
use uuid::Uuid;

use super::{
    error::ErrorResponse,
    organization_members::{ensure_project_access, ensure_swarm_project_access},
};
use crate::{
    AppState, activity::ActivityResponse, auth::RequestContext, db::activity::ActivityRepository,
};

pub fn router() -> Router<AppState> {
    Router::new().route("/activity", get(get_activity_stream))
}

#[derive(Debug, Deserialize)]
pub struct ActivityQuery {
    /// Legacy project ID (deprecated, use swarm_project_id instead)
    pub project_id: Option<Uuid>,
    /// Swarm project ID (preferred)
    pub swarm_project_id: Option<Uuid>,
    /// Fetch events after this ID (exclusive)
    pub after: Option<i64>,
    /// Maximum number of events to return
    pub limit: Option<i64>,
}

#[instrument(
    name = "activity.get_activity_stream",
    skip(state, ctx, params),
    fields(user_id = %ctx.user.id)
)]
async fn get_activity_stream(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
    Query(params): Query<ActivityQuery>,
) -> Response {
    let config = state.config();

    // Clamp limit to configured bounds
    let limit = params
        .limit
        .unwrap_or(config.activity_default_limit)
        .clamp(1, config.activity_max_limit);

    let after = params.after;

    // Perform access check based on which parameter is provided
    // Priority: swarm_project_id > project_id
    let _organization_id = if let Some(swarm_id) = params.swarm_project_id {
        // New path: use swarm project access check
        match ensure_swarm_project_access(state.pool(), ctx.user.id, swarm_id).await {
            Ok(org_id) => org_id,
            Err(error) => return error.into_response(),
        }
    } else if let Some(proj_id) = params.project_id {
        // Legacy path: use project access check
        match ensure_project_access(state.pool(), ctx.user.id, proj_id).await {
            Ok(org_id) => org_id,
            Err(error) => return error.into_response(),
        }
    } else {
        // No parameter provided - return error
        return ErrorResponse::new(
            StatusCode::BAD_REQUEST,
            "either project_id or swarm_project_id is required",
        )
        .into_response();
    };

    // Query activity using the appropriate method based on parameter type
    let repo = ActivityRepository::new(state.pool());

    let events = if params.swarm_project_id.is_some() {
        // Use swarm_project_id query method
        let swarm_id = params.swarm_project_id.unwrap();
        match repo
            .fetch_since_by_swarm_project(swarm_id, after, limit)
            .await
        {
            Ok(events) => events,
            Err(error) => {
                tracing::error!(?error, %swarm_id, "failed to load activity stream by swarm project");
                return ErrorResponse::new(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "failed to load activity stream",
                )
                .into_response();
            }
        }
    } else {
        // Use legacy project_id query method
        let project_id = params.project_id.unwrap();
        match repo.fetch_since(project_id, after, limit).await {
            Ok(events) => events,
            Err(error) => {
                tracing::error!(?error, %project_id, "failed to load activity stream");
                return ErrorResponse::new(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "failed to load activity stream",
                )
                .into_response();
            }
        }
    };

    (StatusCode::OK, Json(ActivityResponse { data: events })).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_activity_query_struct_fields() {
        // Verify that ActivityQuery struct has the expected fields
        let query = ActivityQuery {
            project_id: None,
            swarm_project_id: None,
            after: None,
            limit: None,
        };

        assert!(query.project_id.is_none());
        assert!(query.swarm_project_id.is_none());
        assert!(query.after.is_none());
        assert!(query.limit.is_none());
    }

    #[test]
    fn test_activity_query_optional_fields() {
        // Verify that ActivityQuery fields can be Some
        let project_id = Uuid::new_v4();
        let swarm_project_id = Uuid::new_v4();

        let query = ActivityQuery {
            project_id: Some(project_id),
            swarm_project_id: Some(swarm_project_id),
            after: Some(123),
            limit: Some(100),
        };

        assert_eq!(query.project_id, Some(project_id));
        assert_eq!(query.swarm_project_id, Some(swarm_project_id));
        assert_eq!(query.after, Some(123));
        assert_eq!(query.limit, Some(100));
    }

    #[test]
    fn test_error_response_bad_request() {
        // Verify that ErrorResponse can be created for bad request
        let _error = ErrorResponse::new(
            StatusCode::BAD_REQUEST,
            "either project_id or swarm_project_id is required",
        );

        // ErrorResponse is successfully created with the message
        // (fields are private, so we can't directly assert on them)
    }

    #[test]
    fn test_ensure_swarm_project_access_imported() {
        // Verify that ensure_swarm_project_access is accessible
        // This is a compile-time check; if it fails, the import is missing
        let _fn = ensure_swarm_project_access;
        let _ = _fn; // Use the variable to avoid unused warning
    }
}
