//! WebSocket and REST endpoints for terminal sessions.
//!
//! This module provides:
//! - REST API for creating and listing terminal sessions
//! - WebSocket endpoint for bidirectional terminal I/O

use std::sync::Arc;

use axum::{
    Router,
    extract::{
        Path, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    response::{IntoResponse, Json as ResponseJson},
    routing::{get, post},
};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use services::services::terminal_session::{SessionInfo, TerminalError, TerminalSessionManager};
use tokio::sync::RwLock;
use ts_rs::TS;
use utils::response::ApiResponse;

use crate::{DeploymentImpl, error::ApiError, ws_util::WsKeepAlive};

/// Request body for creating a terminal session.
#[derive(Debug, Deserialize)]
pub struct CreateSessionRequest {
    /// The working directory for the terminal session.
    pub working_dir: String,
}

/// Response for creating a terminal session.
#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct CreateSessionResponse {
    /// The session ID to use for WebSocket connection.
    pub session_id: String,
    /// Information about the created session.
    pub session: SessionInfo,
}

/// WebSocket message types for terminal I/O.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
#[ts(export)]
pub enum TerminalMessage {
    /// Input from the client to send to the terminal.
    Input { data: String },
    /// Resize the terminal dimensions.
    Resize { cols: u16, rows: u16 },
    /// Output from the terminal to the client.
    Output { data: String },
    /// Terminal session has exited.
    Exit { code: Option<i32> },
    /// Error message.
    Error { message: String },
}

/// Shared state for terminal sessions.
pub struct TerminalState {
    pub manager: Arc<RwLock<TerminalSessionManager>>,
}

impl TerminalState {
    pub async fn new() -> Self {
        let mut manager = TerminalSessionManager::new();
        manager.init().await;
        Self {
            manager: Arc::new(RwLock::new(manager)),
        }
    }
}

/// POST /api/terminal/sessions
///
/// Create a new terminal session in the specified working directory.
pub async fn create_session(
    State(terminal_state): State<Arc<TerminalState>>,
    ResponseJson(request): ResponseJson<CreateSessionRequest>,
) -> Result<ResponseJson<ApiResponse<CreateSessionResponse>>, ApiError> {
    let working_dir = std::path::Path::new(&request.working_dir);

    // Validate that the directory exists
    if !working_dir.exists() {
        return Err(ApiError::BadRequest(format!(
            "Working directory does not exist: {}",
            request.working_dir
        )));
    }

    if !working_dir.is_dir() {
        return Err(ApiError::BadRequest(format!(
            "Path is not a directory: {}",
            request.working_dir
        )));
    }

    let manager = terminal_state.manager.read().await;

    // Try to create session, or recreate if unhealthy, or get existing healthy one
    let session_id = match manager.create_or_recreate_session(working_dir).await {
        Ok(id) => id,
        Err(TerminalError::SessionAlreadyExists(id)) => {
            // Session already exists and is healthy - return it
            // This enables seamless reconnection after page refresh
            tracing::info!(session_id = %id, "Reusing existing healthy terminal session");
            id
        }
        Err(e) => {
            return Err(ApiError::BadRequest(e.to_string()));
        }
    };

    let session = manager.get_session(&session_id).await.ok_or_else(|| {
        ApiError::BadRequest("Failed to get session info after creation".to_string())
    })?;

    let response = CreateSessionResponse {
        session_id,
        session,
    };

    Ok(ResponseJson(ApiResponse::success(response)))
}

/// GET /api/terminal/sessions
///
/// List all active terminal sessions.
pub async fn list_sessions(
    State(terminal_state): State<Arc<TerminalState>>,
) -> Result<ResponseJson<ApiResponse<Vec<SessionInfo>>>, ApiError> {
    let manager = terminal_state.manager.read().await;
    let sessions = manager.list_sessions().await;
    Ok(ResponseJson(ApiResponse::success(sessions)))
}

/// GET /api/terminal/sessions/{session_id}
///
/// Get information about a specific terminal session.
pub async fn get_session(
    State(terminal_state): State<Arc<TerminalState>>,
    Path(session_id): Path<String>,
) -> Result<ResponseJson<ApiResponse<SessionInfo>>, ApiError> {
    let manager = terminal_state.manager.read().await;

    let session = manager.get_session(&session_id).await.ok_or_else(|| {
        ApiError::BadRequest(format!("Session not found: {}", session_id))
    })?;

    Ok(ResponseJson(ApiResponse::success(session)))
}

/// DELETE /api/terminal/sessions/{session_id}
///
/// Kill a terminal session.
pub async fn delete_session(
    State(terminal_state): State<Arc<TerminalState>>,
    Path(session_id): Path<String>,
) -> Result<ResponseJson<ApiResponse<()>>, ApiError> {
    let manager = terminal_state.manager.read().await;

    manager
        .kill_session(&session_id)
        .await
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    Ok(ResponseJson(ApiResponse::success(())))
}

/// WS /api/terminal/ws/{session_id}
///
/// WebSocket endpoint for bidirectional terminal I/O.
pub async fn terminal_websocket(
    ws: WebSocketUpgrade,
    State(terminal_state): State<Arc<TerminalState>>,
    Path(session_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    // Verify session exists
    {
        let manager = terminal_state.manager.read().await;
        if !manager.session_exists(&session_id) {
            return Err(ApiError::BadRequest(format!(
                "Session not found: {}",
                session_id
            )));
        }
    }

    Ok(ws.on_upgrade(move |socket| async move {
        if let Err(e) = handle_terminal_ws(socket, terminal_state, session_id.clone()).await {
            tracing::warn!(session_id = %session_id, error = %e, "terminal WebSocket closed with error");
        }
    }))
}

/// Handle a terminal WebSocket connection.
async fn handle_terminal_ws(
    socket: WebSocket,
    terminal_state: Arc<TerminalState>,
    session_id: String,
) -> anyhow::Result<()> {
    let (mut sender, mut receiver) = socket.split();

    // Subscribe to terminal output
    let output_rx = {
        let manager = terminal_state.manager.read().await;
        manager.subscribe(&session_id).await.map_err(|e| {
            anyhow::anyhow!("Failed to subscribe to session output: {}", e)
        })?
    };

    let keep_alive = WsKeepAlive::for_execution_streams();

    let mut ping_interval = tokio::time::interval(keep_alive.ping_interval);
    ping_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    let mut last_pong = tokio::time::Instant::now();

    // Convert output receiver to a stream
    let mut output_rx = tokio_stream::wrappers::BroadcastStream::new(output_rx);

    loop {
        tokio::select! {
            // Handle terminal output
            output = output_rx.next() => {
                match output {
                    Some(Ok(terminal_output)) => {
                        let msg = TerminalMessage::Output { data: terminal_output.data };
                        let json = serde_json::to_string(&msg)?;
                        if sender.send(Message::Text(json.into())).await.is_err() {
                            tracing::debug!(session_id = %session_id, "client disconnected during send");
                            break;
                        }
                    }
                    Some(Err(e)) => {
                        tracing::debug!(session_id = %session_id, error = ?e, "output stream error");
                        // Lagged - skip some messages and continue
                        continue;
                    }
                    None => {
                        tracing::debug!(session_id = %session_id, "output stream ended");
                        // Send exit message
                        let msg = TerminalMessage::Exit { code: None };
                        let json = serde_json::to_string(&msg)?;
                        let _ = sender.send(Message::Text(json.into())).await;
                        break;
                    }
                }
            }

            // Handle incoming WebSocket messages
            msg = receiver.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        match serde_json::from_str::<TerminalMessage>(&text) {
                            Ok(terminal_msg) => {
                                if let Err(e) = handle_client_message(
                                    &terminal_state,
                                    &session_id,
                                    terminal_msg,
                                ).await {
                                    tracing::warn!(
                                        session_id = %session_id,
                                        error = %e,
                                        "error handling client message"
                                    );
                                    // Send error message to client
                                    let error_msg = TerminalMessage::Error {
                                        message: e.to_string(),
                                    };
                                    let json = serde_json::to_string(&error_msg)?;
                                    let _ = sender.send(Message::Text(json.into())).await;
                                }
                            }
                            Err(e) => {
                                tracing::debug!(
                                    session_id = %session_id,
                                    error = %e,
                                    "invalid JSON message from client"
                                );
                            }
                        }
                    }
                    Some(Ok(Message::Pong(_))) => {
                        last_pong = tokio::time::Instant::now();
                    }
                    Some(Ok(Message::Ping(data))) => {
                        if sender.send(Message::Pong(data)).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(Message::Close(_))) => {
                        tracing::debug!(session_id = %session_id, "client sent close frame");
                        break;
                    }
                    Some(Err(e)) => {
                        tracing::debug!(session_id = %session_id, error = ?e, "WebSocket receive error");
                        break;
                    }
                    None => {
                        tracing::debug!(session_id = %session_id, "WebSocket stream ended");
                        break;
                    }
                    _ => {} // Ignore binary messages
                }
            }

            // Send ping and check pong timeout
            _ = ping_interval.tick() => {
                if last_pong.elapsed() > keep_alive.pong_timeout {
                    tracing::warn!(
                        session_id = %session_id,
                        elapsed_secs = last_pong.elapsed().as_secs(),
                        "WebSocket pong timeout, closing connection"
                    );
                    break;
                }

                if sender.send(Message::Ping(Vec::new().into())).await.is_err() {
                    tracing::debug!(session_id = %session_id, "failed to send ping, client disconnected");
                    break;
                }
            }
        }
    }

    // Attempt graceful close
    let _ = sender.send(Message::Close(None)).await;

    Ok(())
}

/// Handle a message from the client.
async fn handle_client_message(
    terminal_state: &Arc<TerminalState>,
    session_id: &str,
    msg: TerminalMessage,
) -> Result<(), TerminalError> {
    let manager = terminal_state.manager.read().await;

    match msg {
        TerminalMessage::Input { data } => {
            manager.write_to_session(session_id, data.as_bytes()).await?;
        }
        TerminalMessage::Resize { cols, rows } => {
            manager.resize_session(session_id, cols, rows).await?;
        }
        // Output, Exit, and Error are server-to-client messages
        _ => {}
    }

    Ok(())
}

/// Create the router for terminal endpoints.
///
/// Note: This router requires terminal state to be added separately.
/// Use `router_with_state` to get a fully configured router.
pub fn router() -> Router<Arc<TerminalState>> {
    Router::new()
        .route("/terminal/sessions", post(create_session))
        .route("/terminal/sessions", get(list_sessions))
        .route("/terminal/sessions/{session_id}", get(get_session))
        .route(
            "/terminal/sessions/{session_id}",
            axum::routing::delete(delete_session),
        )
        .route("/terminal/ws/{session_id}", get(terminal_websocket))
}

/// Create a router with its own terminal state.
///
/// This is a convenience function that creates the terminal state and
/// returns a router that can be merged into the main app router.
pub async fn router_with_state(_deployment: &DeploymentImpl) -> Router<DeploymentImpl> {
    let terminal_state = Arc::new(TerminalState::new().await);

    Router::new()
        .route("/terminal/sessions", post(create_session))
        .route("/terminal/sessions", get(list_sessions))
        .route("/terminal/sessions/{session_id}", get(get_session))
        .route(
            "/terminal/sessions/{session_id}",
            axum::routing::delete(delete_session),
        )
        .route("/terminal/ws/{session_id}", get(terminal_websocket))
        .with_state(terminal_state)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_terminal_message_serialization() {
        // Test Input message
        let input_msg = TerminalMessage::Input {
            data: "ls -la\n".to_string(),
        };
        let json = serde_json::to_string(&input_msg).unwrap();
        assert!(json.contains("\"type\":\"input\""));
        assert!(json.contains("\"data\":\"ls -la\\n\""));

        // Test Output message
        let output_msg = TerminalMessage::Output {
            data: "file1.txt\nfile2.txt\n".to_string(),
        };
        let json = serde_json::to_string(&output_msg).unwrap();
        assert!(json.contains("\"type\":\"output\""));

        // Test Resize message
        let resize_msg = TerminalMessage::Resize { cols: 120, rows: 40 };
        let json = serde_json::to_string(&resize_msg).unwrap();
        assert!(json.contains("\"type\":\"resize\""));
        assert!(json.contains("\"cols\":120"));
        assert!(json.contains("\"rows\":40"));

        // Test Exit message
        let exit_msg = TerminalMessage::Exit { code: Some(0) };
        let json = serde_json::to_string(&exit_msg).unwrap();
        assert!(json.contains("\"type\":\"exit\""));
        assert!(json.contains("\"code\":0"));

        // Test Error message
        let error_msg = TerminalMessage::Error {
            message: "Session not found".to_string(),
        };
        let json = serde_json::to_string(&error_msg).unwrap();
        assert!(json.contains("\"type\":\"error\""));
    }

    #[test]
    fn test_terminal_message_deserialization() {
        // Test Input message
        let json = r#"{"type":"input","data":"echo hello"}"#;
        let msg: TerminalMessage = serde_json::from_str(json).unwrap();
        match msg {
            TerminalMessage::Input { data } => assert_eq!(data, "echo hello"),
            _ => panic!("Expected Input message"),
        }

        // Test Resize message
        let json = r#"{"type":"resize","cols":80,"rows":24}"#;
        let msg: TerminalMessage = serde_json::from_str(json).unwrap();
        match msg {
            TerminalMessage::Resize { cols, rows } => {
                assert_eq!(cols, 80);
                assert_eq!(rows, 24);
            }
            _ => panic!("Expected Resize message"),
        }
    }

    #[test]
    fn test_create_session_response_serialization() {
        let response = CreateSessionResponse {
            session_id: "vk-abc12345".to_string(),
            session: SessionInfo {
                id: "vk-abc12345".to_string(),
                working_dir: "/home/user/project".to_string(),
                is_tmux: true,
                cols: 80,
                rows: 24,
                active: true,
            },
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"session_id\":\"vk-abc12345\""));
        assert!(json.contains("\"is_tmux\":true"));
    }
}
