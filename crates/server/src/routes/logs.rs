//! REST endpoint for paginated log access.
//!
//! This module provides a unified REST API for fetching log entries with cursor-based
//! pagination. It works for both local and remote executions.
//!
//! Note: Gzip compression can be enabled at the reverse proxy level (nginx, etc.)
//! or by adding tower-http CompressionLayer in a future enhancement.

use axum::{
    Router,
    extract::{Path, Query, State},
    response::Json as ResponseJson,
    routing::get,
};
use deployment::Deployment;
use serde::Deserialize;
use services::services::unified_logs::{LogServiceError, UnifiedLogService};
use utils::{
    response::ApiResponse,
    unified_log::{Direction, PaginatedLogs},
};
use uuid::Uuid;

use crate::{DeploymentImpl, error::ApiError};

/// Default number of log entries to return per page.
const DEFAULT_LIMIT: i64 = 100;

/// Maximum number of log entries to return per page.
const MAX_LIMIT: i64 = 500;

/// Query parameters for the paginated logs endpoint.
#[derive(Debug, Deserialize)]
pub struct PaginationParams {
    /// Maximum number of entries to return. Defaults to 100, max 500.
    #[serde(default)]
    pub limit: Option<i64>,

    /// Cursor (entry ID) to start from. If not provided, starts from the beginning
    /// (for Forward) or end (for Backward).
    #[serde(default)]
    pub cursor: Option<i64>,

    /// Direction of pagination. Defaults to "backward" (newest first).
    #[serde(default)]
    pub direction: Option<Direction>,
}

impl PaginationParams {
    /// Get the limit, clamped between 1 and MAX_LIMIT.
    pub fn limit(&self) -> i64 {
        self.limit
            .unwrap_or(DEFAULT_LIMIT)
            .clamp(1, MAX_LIMIT)
    }

    /// Get the direction, defaulting to Backward (newest first) for initial loads.
    pub fn direction(&self) -> Direction {
        self.direction.unwrap_or(Direction::Backward)
    }
}

/// GET /api/logs/{execution_id}
///
/// Returns paginated log entries for the specified execution.
///
/// # Query Parameters
/// - `limit`: Maximum entries to return (default: 100, max: 500)
/// - `cursor`: Entry ID to start from (for pagination)
/// - `direction`: "forward" (oldest first) or "backward" (newest first, default)
///
/// # Response
/// Returns a `PaginatedLogs` object containing:
/// - `entries`: Array of LogEntry objects
/// - `next_cursor`: Cursor for the next page (if more entries exist)
/// - `has_more`: Boolean indicating if more entries are available
/// - `total_count`: Total number of log entries (if available)
pub async fn get_logs(
    State(deployment): State<DeploymentImpl>,
    Path(execution_id): Path<Uuid>,
    Query(params): Query<PaginationParams>,
) -> Result<ResponseJson<ApiResponse<PaginatedLogs>>, ApiError> {
    let limit = params.limit();
    let cursor = params.cursor;
    let direction = params.direction();

    // Create the unified log service
    let service = UnifiedLogService::new(
        deployment.db().pool.clone(),
        deployment.node_proxy_client().clone(),
    );

    // Fetch paginated logs
    let paginated = service
        .get_logs_paginated(execution_id, cursor, limit, direction)
        .await
        .map_err(|e| match e {
            LogServiceError::ExecutionNotFound(id) => {
                ApiError::BadRequest(format!("Execution not found: {}", id))
            }
            LogServiceError::Database(db_err) => ApiError::Database(db_err),
            LogServiceError::RemoteProxy(proxy_err) => ApiError::NodeProxy(proxy_err),
            LogServiceError::InvalidLocation => {
                ApiError::BadRequest("Invalid execution location".to_string())
            }
        })?;

    Ok(ResponseJson(ApiResponse::success(paginated)))
}

/// Create the router for log endpoints.
pub fn router(deployment: &DeploymentImpl) -> Router<DeploymentImpl> {
    let _ = deployment; // Reserved for future middleware
    Router::new().route("/logs/{execution_id}", get(get_logs))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pagination_params_defaults() {
        let params = PaginationParams {
            limit: None,
            cursor: None,
            direction: None,
        };

        assert_eq!(params.limit(), DEFAULT_LIMIT);
        assert_eq!(params.direction(), Direction::Backward);
    }

    #[test]
    fn test_pagination_params_limit_clamping() {
        // Test upper bound
        let params = PaginationParams {
            limit: Some(9999),
            cursor: None,
            direction: None,
        };
        assert_eq!(params.limit(), MAX_LIMIT);

        // Test lower bound
        let params = PaginationParams {
            limit: Some(0),
            cursor: None,
            direction: None,
        };
        assert_eq!(params.limit(), 1);

        // Test negative value
        let params = PaginationParams {
            limit: Some(-10),
            cursor: None,
            direction: None,
        };
        assert_eq!(params.limit(), 1);
    }

    #[test]
    fn test_pagination_params_direction() {
        // Forward direction
        let params = PaginationParams {
            limit: None,
            cursor: None,
            direction: Some(Direction::Forward),
        };
        assert_eq!(params.direction(), Direction::Forward);

        // Backward direction
        let params = PaginationParams {
            limit: None,
            cursor: None,
            direction: Some(Direction::Backward),
        };
        assert_eq!(params.direction(), Direction::Backward);
    }

    #[test]
    fn test_pagination_params_custom_values() {
        let params = PaginationParams {
            limit: Some(50),
            cursor: Some(100),
            direction: Some(Direction::Forward),
        };

        assert_eq!(params.limit(), 50);
        assert_eq!(params.cursor, Some(100));
        assert_eq!(params.direction(), Direction::Forward);
    }
}
