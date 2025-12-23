//! WebSocket relay and REST API for log streaming from nodes.
//!
//! This module provides:
//! - REST API for fetching logs with cursor-based pagination
//! - WebSocket endpoint for live log streaming
//!
//! Both endpoints allow frontend clients to access logs from task execution
//! via the Hive when direct node connection is not available.

use axum::{
    Json, Router,
    extract::{
        Extension, Path, Query, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::time::Duration;
use tokio::time::interval;
use tracing::instrument;
use utils::unified_log::{Direction, PaginatedLogs};
use uuid::Uuid;

use crate::{
    AppState,
    auth::{ConnectionTokenError, RequestContext},
    db::{
        task_assignments::TaskAssignmentRepository, task_output_logs::TaskOutputLogRepository,
        tasks::SharedTaskRepository,
    },
    routes::organization_members::ensure_member_access,
};

/// Polling interval for new logs
const LOG_POLL_INTERVAL_MS: u64 = 100;

/// Maximum logs to fetch per poll
const MAX_LOGS_PER_POLL: i64 = 100;

/// Default number of log entries to return per page.
const DEFAULT_LIMIT: i64 = 100;

/// Maximum number of log entries to return per page.
const MAX_LIMIT: i64 = 500;

/// Create the router for relay endpoints.
pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/nodes/assignments/{assignment_id}/logs/ws",
            get(upgrade_log_stream),
        )
        .route("/logs/{assignment_id}", get(get_paginated_logs))
}

#[derive(Debug, Deserialize)]
pub struct LogStreamQuery {
    /// Connection token for authentication
    pub token: Option<String>,
}

/// Query parameters for the paginated logs endpoint.
#[derive(Debug, Deserialize)]
pub struct PaginationQuery {
    /// Maximum number of entries to return. Defaults to 100, max 500.
    pub limit: Option<i64>,

    /// Cursor (entry ID) to start from. If not provided, starts from the beginning
    /// (for Forward) or end (for Backward).
    pub cursor: Option<i64>,

    /// Direction of pagination. Defaults to "backward" (newest first).
    pub direction: Option<Direction>,

    /// Connection token for authentication (optional, for external access)
    pub token: Option<String>,
}

impl PaginationQuery {
    /// Get the limit, clamped between 1 and MAX_LIMIT.
    fn limit(&self) -> i64 {
        self.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT)
    }

    /// Get the direction, defaulting to Backward (newest first) for initial loads.
    fn direction(&self) -> Direction {
        self.direction.unwrap_or(Direction::Backward)
    }
}

/// API response wrapper matching the local server's response format.
#[derive(Debug, Serialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub message: Option<String>,
}

impl<T> ApiResponse<T> {
    pub fn success(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            message: None,
        }
    }

    pub fn error(message: impl Into<String>) -> ApiResponse<()> {
        ApiResponse {
            success: false,
            data: None,
            message: Some(message.into()),
        }
    }
}

/// Log entry sent to clients via WebSocket.
#[derive(Debug, Clone, Serialize)]
pub struct LogStreamEntry {
    pub id: i64,
    pub output_type: String,
    pub content: String,
    pub timestamp: String,
}

/// Messages sent to clients.
#[derive(Debug, Serialize)]
#[serde(tag = "type")]
pub enum LogStreamMessage {
    /// Log entries
    #[serde(rename = "logs")]
    Logs { entries: Vec<LogStreamEntry> },
    /// Heartbeat to keep connection alive
    #[serde(rename = "heartbeat")]
    Heartbeat,
    /// Error message
    #[serde(rename = "error")]
    Error { message: String },
}

/// Handle WebSocket upgrade for log streaming.
///
/// Supports two authentication modes:
/// 1. Connection token in query parameter (for direct access without JWT)
/// 2. User JWT session (via RequestContext extension)
#[instrument(
    name = "relay.upgrade_log_stream",
    skip(state, ws, query, ctx),
    fields(assignment_id = %assignment_id)
)]
pub async fn upgrade_log_stream(
    State(state): State<AppState>,
    ws: WebSocketUpgrade,
    Path(assignment_id): Path<Uuid>,
    Query(query): Query<LogStreamQuery>,
    ctx: Option<Extension<RequestContext>>,
) -> Response {
    let pool = state.pool();

    // Try to authenticate
    let auth_result = authenticate(&state, &query, ctx.as_ref().map(|e| &e.0), assignment_id).await;

    match auth_result {
        Ok(()) => {
            // Verified access, proceed with WebSocket upgrade
            let pool = pool.clone();
            ws.on_upgrade(move |socket| handle_log_stream(socket, pool, assignment_id))
        }
        Err(response) => response,
    }
}

/// Authenticate the request using either connection token or JWT session.
async fn authenticate(
    state: &AppState,
    query: &LogStreamQuery,
    ctx: Option<&RequestContext>,
    assignment_id: Uuid,
) -> Result<(), Response> {
    let pool = state.pool();

    // First, try connection token authentication
    if let Some(token) = &query.token {
        let connection_token_service = state.connection_token();
        match connection_token_service.validate_for_assignment(token, assignment_id) {
            Ok(_) => return Ok(()),
            Err(ConnectionTokenError::ExecutionMismatch) => {
                return Err((
                    StatusCode::FORBIDDEN,
                    Json(json!({ "error": "token does not match assignment" })),
                )
                    .into_response());
            }
            Err(ConnectionTokenError::TokenExpired) => {
                return Err((
                    StatusCode::UNAUTHORIZED,
                    Json(json!({ "error": "token expired" })),
                )
                    .into_response());
            }
            Err(_) => {
                return Err((
                    StatusCode::UNAUTHORIZED,
                    Json(json!({ "error": "invalid token" })),
                )
                    .into_response());
            }
        }
    }

    // Fall back to JWT session authentication
    let ctx = ctx.ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "authentication required" })),
        )
            .into_response()
    })?;

    // Get the assignment
    let assignment_repo = TaskAssignmentRepository::new(pool);
    let assignment = assignment_repo
        .find_by_id(assignment_id)
        .await
        .map_err(|e| {
            tracing::error!(?e, "failed to fetch assignment");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "internal server error" })),
            )
                .into_response()
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "assignment not found" })),
            )
                .into_response()
        })?;

    // Get the task to verify organization access
    let task_repo = SharedTaskRepository::new(pool);
    let task = task_repo
        .find_by_id(assignment.task_id)
        .await
        .map_err(|e| {
            tracing::error!(?e, "failed to fetch task");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "internal server error" })),
            )
                .into_response()
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "task not found" })),
            )
                .into_response()
        })?;

    // Verify user has access to the organization
    ensure_member_access(pool, task.organization_id, ctx.user.id)
        .await
        .map_err(|e| e.into_response())?;

    Ok(())
}

/// Handle the WebSocket connection for log streaming.
async fn handle_log_stream(socket: WebSocket, pool: sqlx::PgPool, assignment_id: Uuid) {
    let (mut sender, mut receiver) = socket.split();

    // Track the last log ID we've sent
    let mut last_log_id: i64 = 0;

    // Poll interval
    let mut poll_interval = interval(Duration::from_millis(LOG_POLL_INTERVAL_MS));
    poll_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    // Heartbeat interval (30 seconds)
    let mut heartbeat_interval = interval(Duration::from_secs(30));
    heartbeat_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            // Poll for new logs
            _ = poll_interval.tick() => {
                let log_repo = TaskOutputLogRepository::new(&pool);

                match log_repo.list_by_assignment(assignment_id, Some(MAX_LOGS_PER_POLL), Some(last_log_id)).await {
                    Ok(logs) => {
                        if !logs.is_empty() {
                            // Update the last log ID
                            if let Some(last) = logs.last() {
                                last_log_id = last.id;
                            }

                            // Convert to stream entries
                            let entries: Vec<LogStreamEntry> = logs.into_iter().map(|log| {
                                LogStreamEntry {
                                    id: log.id,
                                    output_type: log.output_type,
                                    content: log.content,
                                    timestamp: log.timestamp.to_rfc3339(),
                                }
                            }).collect();

                            // Send to client
                            let message = LogStreamMessage::Logs { entries };
                            if let Ok(json) = serde_json::to_string(&message)
                                && sender.send(Message::Text(json.into())).await.is_err()
                            {
                                // Client disconnected
                                break;
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!(?e, "failed to fetch logs");
                        let message = LogStreamMessage::Error { message: "failed to fetch logs".to_string() };
                        if let Ok(json) = serde_json::to_string(&message) {
                            let _ = sender.send(Message::Text(json.into())).await;
                        }
                        break;
                    }
                }
            }

            // Send heartbeat
            _ = heartbeat_interval.tick() => {
                let message = LogStreamMessage::Heartbeat;
                if let Ok(json) = serde_json::to_string(&message)
                    && sender.send(Message::Text(json.into())).await.is_err()
                {
                    break;
                }
            }

            // Handle incoming messages (client might send close or ping)
            msg = receiver.next() => {
                match msg {
                    Some(Ok(Message::Close(_))) | None => {
                        break;
                    }
                    Some(Ok(Message::Ping(data))) => {
                        if sender.send(Message::Pong(data)).await.is_err() {
                            break;
                        }
                    }
                    Some(Err(_)) => {
                        break;
                    }
                    _ => {}
                }
            }
        }
    }

    tracing::debug!(%assignment_id, "log stream closed");
}

/// GET /v1/logs/{assignment_id}
///
/// Returns paginated log entries for the specified assignment.
///
/// # Query Parameters
/// - `limit`: Maximum entries to return (default: 100, max: 500)
/// - `cursor`: Entry ID to start from (for pagination)
/// - `direction`: "forward" (oldest first) or "backward" (newest first, default)
/// - `token`: Connection token for authentication (optional)
///
/// # Response
/// Returns a `PaginatedLogs` object containing:
/// - `entries`: Array of LogEntry objects
/// - `next_cursor`: Cursor for the next page (if more entries exist)
/// - `has_more`: Boolean indicating if more entries are available
/// - `total_count`: Total number of log entries (if available)
#[instrument(
    name = "relay.get_paginated_logs",
    skip(state, query, ctx),
    fields(assignment_id = %assignment_id)
)]
pub async fn get_paginated_logs(
    State(state): State<AppState>,
    Path(assignment_id): Path<Uuid>,
    Query(query): Query<PaginationQuery>,
    ctx: Option<Extension<RequestContext>>,
) -> Response {
    // Convert to LogStreamQuery for authentication
    let auth_query = LogStreamQuery {
        token: query.token.clone(),
    };

    // Try to authenticate
    let auth_result = authenticate(
        &state,
        &auth_query,
        ctx.as_ref().map(|e| &e.0),
        assignment_id,
    )
    .await;

    if let Err(response) = auth_result {
        return response;
    }

    let limit = query.limit();
    let cursor = query.cursor;
    let direction = query.direction();
    let cache = state.log_cache();

    // Check cache first
    if let Some(cached) = cache.get(assignment_id, cursor, limit, direction) {
        tracing::debug!(%assignment_id, "cache hit for paginated logs");
        return (StatusCode::OK, Json(ApiResponse::success(cached))).into_response();
    }

    // Cache miss - fetch from database
    let pool = state.pool();
    let log_repo = TaskOutputLogRepository::new(pool);

    match log_repo
        .find_paginated_with_count(assignment_id, cursor, limit, direction)
        .await
    {
        Ok(paginated) => {
            // Cache the result
            cache.set(assignment_id, cursor, limit, direction, paginated.clone());

            tracing::debug!(
                %assignment_id,
                entries = paginated.entries.len(),
                has_more = paginated.has_more,
                "fetched paginated logs"
            );

            (StatusCode::OK, Json(ApiResponse::success(paginated))).into_response()
        }
        Err(e) => {
            tracing::error!(?e, "failed to fetch paginated logs");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::<PaginatedLogs>::error("failed to fetch logs")),
            )
                .into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pagination_query_defaults() {
        let query = PaginationQuery {
            limit: None,
            cursor: None,
            direction: None,
            token: None,
        };

        assert_eq!(query.limit(), DEFAULT_LIMIT);
        assert_eq!(query.direction(), Direction::Backward);
    }

    #[test]
    fn test_pagination_query_limit_clamping() {
        // Test upper bound
        let query = PaginationQuery {
            limit: Some(9999),
            cursor: None,
            direction: None,
            token: None,
        };
        assert_eq!(query.limit(), MAX_LIMIT);

        // Test lower bound
        let query = PaginationQuery {
            limit: Some(0),
            cursor: None,
            direction: None,
            token: None,
        };
        assert_eq!(query.limit(), 1);

        // Test negative value
        let query = PaginationQuery {
            limit: Some(-10),
            cursor: None,
            direction: None,
            token: None,
        };
        assert_eq!(query.limit(), 1);
    }

    #[test]
    fn test_pagination_query_direction() {
        let query = PaginationQuery {
            limit: None,
            cursor: None,
            direction: Some(Direction::Forward),
            token: None,
        };
        assert_eq!(query.direction(), Direction::Forward);

        let query = PaginationQuery {
            limit: None,
            cursor: None,
            direction: Some(Direction::Backward),
            token: None,
        };
        assert_eq!(query.direction(), Direction::Backward);
    }
}
