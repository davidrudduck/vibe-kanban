//! REST and WebSocket endpoints for unified log access.
//!
//! This module provides:
//! - REST API for fetching log entries with cursor-based pagination
//! - WebSocket endpoint for live-only log streaming
//!
//! Both endpoints work for local and remote executions.
//!
//! Note: Gzip compression can be enabled at the reverse proxy level (nginx, etc.)
//! or by adding tower-http CompressionLayer in a future enhancement.

use axum::{
    Router,
    extract::{
        Path, Query, State,
        ws::{WebSocket, WebSocketUpgrade},
    },
    response::{IntoResponse, Json as ResponseJson},
    routing::get,
};
use db::models::execution_process::ExecutionProcessError;
use deployment::Deployment;
use futures_util::TryStreamExt;
use serde::Deserialize;
use services::services::{
    container::ContainerService,
    unified_logs::{LogServiceError, UnifiedLogService},
};
use utils::{
    log_msg::LogMsg,
    response::ApiResponse,
    unified_log::{Direction, PaginatedLogs},
};
use uuid::Uuid;

use crate::{
    DeploymentImpl,
    error::ApiError,
    ws_util::{WsKeepAlive, run_ws_stream},
};

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
        self.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT)
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

/// Query parameters for the live logs WebSocket endpoint.
#[derive(Debug, Deserialize)]
pub struct LiveLogStreamQuery {
    /// Optional connection token for external access (e.g., from Hive frontend)
    pub token: Option<String>,
}

/// WS /api/logs/{execution_id}/live
///
/// WebSocket endpoint for live-only log streaming.
/// This endpoint streams only new log entries as they are produced,
/// without replaying history. Use the REST endpoint for paginated history.
///
/// # Authentication
/// Supports two authentication modes:
/// 1. Session-based (local access) - no token required
/// 2. Token-based (external access) - connection token in query param
pub async fn stream_live_logs_ws(
    ws: WebSocketUpgrade,
    State(deployment): State<DeploymentImpl>,
    Path(execution_id): Path<Uuid>,
    Query(query): Query<LiveLogStreamQuery>,
) -> Result<impl IntoResponse, ApiError> {
    // If a token is provided, validate it
    if let Some(token) = &query.token {
        let validator = deployment.connection_token_validator();

        if !validator.is_enabled() {
            tracing::warn!(
                "connection token provided but validation is disabled (VK_CONNECTION_TOKEN_SECRET not set)"
            );
            return Err(ApiError::Forbidden(
                "connection token validation not configured on this node".to_string(),
            ));
        }

        match validator.validate_for_execution(token, execution_id) {
            Ok(validated) => {
                tracing::debug!(
                    user_id = %validated.user_id,
                    exec_id = %execution_id,
                    "connection token validated for live log streaming"
                );
            }
            Err(e) => {
                tracing::warn!(?e, "invalid connection token for live log streaming");
                return Err(ApiError::Unauthorized);
            }
        }
    }

    // Check if the execution exists and get stream
    let stream = deployment
        .container()
        .stream_live_logs_only(&execution_id)
        .await
        .ok_or_else(|| {
            ApiError::ExecutionProcess(ExecutionProcessError::ExecutionProcessNotFound)
        })?;

    Ok(ws.on_upgrade(move |socket| async move {
        if let Err(e) = handle_live_logs_ws(socket, stream).await {
            tracing::warn!(exec_id = %execution_id, "live logs WS closed: {}", e);
        }
    }))
}

async fn handle_live_logs_ws(
    socket: WebSocket,
    stream: impl futures_util::Stream<Item = std::io::Result<LogMsg>> + Unpin + Send + 'static,
) -> anyhow::Result<()> {
    // Convert LogMsg to WebSocket messages
    let stream = stream.map_ok(|msg| msg.to_ws_message_unchecked());

    // Use run_ws_stream for proper keep-alive handling
    run_ws_stream(socket, stream, WsKeepAlive::for_execution_streams()).await
}

/// Create the router for log endpoints.
pub fn router(deployment: &DeploymentImpl) -> Router<DeploymentImpl> {
    let _ = deployment; // Reserved for future middleware
    Router::new()
        .route("/logs/{execution_id}", get(get_logs))
        .route("/logs/{execution_id}/live", get(stream_live_logs_ws))
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
