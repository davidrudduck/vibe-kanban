use std::path::PathBuf;

use axum::{
    Router,
    extract::{Query, State, ws::Message},
    response::IntoResponse,
    routing::get,
};
use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use db::models::{
    execution_process::{ExecutionProcess, ExecutionProcessStatus},
    workspace::Workspace,
    workspace_repo::WorkspaceRepo,
};
use deployment::Deployment;
use executors::executors::BaseCodingAgent;
use local_deployment::pty::build_tmux_attach_command;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    DeploymentImpl,
    error::ApiError,
    middleware::signed_ws::{MaybeSignedWebSocket, SignedWsUpgrade},
};

#[derive(Debug, Deserialize)]
struct TerminalQuery {
    pub workspace_id: Uuid,
    #[serde(default)]
    pub tmux_session: Option<String>,
    #[serde(default = "default_cols")]
    pub cols: u16,
    #[serde(default = "default_rows")]
    pub rows: u16,
}

fn default_cols() -> u16 {
    80
}

fn default_rows() -> u16 {
    24
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum TerminalCommand {
    Input { data: String },
    Resize { cols: u16, rows: u16 },
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum TerminalMessage {
    Output { data: String },
    Error { message: String },
}

async fn terminal_ws(
    ws: SignedWsUpgrade,
    State(deployment): State<DeploymentImpl>,
    Query(query): Query<TerminalQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let attempt = Workspace::find_by_id(&deployment.db().pool, query.workspace_id)
        .await?
        .ok_or_else(|| ApiError::BadRequest("Attempt not found".to_string()))?;

    let container_ref = attempt
        .container_ref
        .ok_or_else(|| ApiError::BadRequest("Attempt has no workspace directory".to_string()))?;

    let base_dir = PathBuf::from(&container_ref);
    if !base_dir.exists() {
        return Err(ApiError::BadRequest(
            "Workspace directory does not exist".to_string(),
        ));
    }

    if let Some(tmux_session) = query.tmux_session.as_deref() {
        validate_tmux_session_for_workspace(&deployment, attempt.id, tmux_session).await?;
    }

    let mut working_dir = base_dir.clone();
    match WorkspaceRepo::find_repos_for_workspace(&deployment.db().pool, query.workspace_id).await {
        Ok(repos) if repos.len() == 1 => {
            let repo_dir = base_dir.join(&repos[0].name);
            if repo_dir.exists() {
                working_dir = repo_dir;
            }
        }
        Ok(_) => {}
        Err(e) => {
            tracing::warn!(
                "Failed to resolve repos for workspace {}: {}",
                attempt.id,
                e
            );
        }
    }

    Ok(ws.on_upgrade(move |socket| {
        handle_terminal_ws(
            socket,
            deployment,
            working_dir,
            query.cols,
            query.rows,
            query.tmux_session,
        )
    }))
}

async fn validate_tmux_session_for_workspace(
    deployment: &DeploymentImpl,
    workspace_id: Uuid,
    tmux_session: &str,
) -> Result<(), ApiError> {
    let execution_id = tmux_session
        .strip_prefix("vk-claude-")
        .and_then(|id| Uuid::parse_str(id).ok())
        .ok_or_else(|| ApiError::BadRequest("Unsupported tmux session".to_string()))?;

    let process = ExecutionProcess::find_by_id(&deployment.db().pool, execution_id)
        .await?
        .ok_or_else(|| ApiError::BadRequest("Tmux session execution not found".to_string()))?;
    if process.status != ExecutionProcessStatus::Running {
        return Err(ApiError::BadRequest(
            "Tmux session execution is not running".to_string(),
        ));
    }
    let executor = process
        .executor_action()
        .map_err(|err| ApiError::BadRequest(err.to_string()))?
        .base_executor();
    if executor != Some(BaseCodingAgent::ClaudeTerminal) {
        return Err(ApiError::BadRequest(
            "Tmux session is not a Claude terminal execution".to_string(),
        ));
    }
    let Some((workspace, _)) = process
        .parent_workspace_and_session(&deployment.db().pool)
        .await?
    else {
        return Err(ApiError::BadRequest(
            "Tmux session workspace not found".to_string(),
        ));
    };

    if workspace.id != workspace_id {
        return Err(ApiError::BadRequest(
            "Tmux session does not belong to workspace".to_string(),
        ));
    }

    Ok(())
}

async fn handle_terminal_ws(
    mut socket: MaybeSignedWebSocket,
    deployment: DeploymentImpl,
    working_dir: PathBuf,
    cols: u16,
    rows: u16,
    tmux_session: Option<String>,
) {
    let create_result = if let Some(tmux_session) = tmux_session {
        if !tmux_session.starts_with("vk-claude-") {
            let _ = send_error(&mut socket, "Unsupported tmux session").await;
            return;
        }
        deployment
            .pty()
            .create_command_session(
                working_dir,
                cols,
                rows,
                build_tmux_attach_command(&tmux_session),
            )
            .await
    } else {
        deployment
            .pty()
            .create_session(working_dir, cols, rows)
            .await
    };

    let (session_id, mut output_rx) = match create_result {
        Ok(result) => result,
        Err(e) => {
            tracing::error!("Failed to create PTY session: {}", e);
            let _ = send_error(&mut socket, &e.to_string()).await;
            return;
        }
    };

    let pty_service = deployment.pty().clone();
    let session_id_for_input = session_id;

    loop {
        tokio::select! {
            maybe_output = output_rx.recv() => {
                let Some(data) = maybe_output else {
                    break;
                };

                let msg = TerminalMessage::Output {
                    data: BASE64.encode(&data),
                };
                let json = match serde_json::to_string(&msg) {
                    Ok(j) => j,
                    Err(_) => continue,
                };

                if socket.send(Message::Text(json.into())).await.is_err() {
                    break;
                }
            }
            inbound = socket.recv() => {
                match inbound {
                    Ok(Some(Message::Text(text))) => {
                        if let Ok(cmd) = serde_json::from_str::<TerminalCommand>(text.as_str()) {
                            match cmd {
                                TerminalCommand::Input { data } => {
                                    if let Ok(bytes) = BASE64.decode(&data) {
                                        let _ = pty_service.write(session_id_for_input, &bytes).await;
                                    }
                                }
                                TerminalCommand::Resize { cols, rows } => {
                                    let _ = pty_service.resize(session_id_for_input, cols, rows).await;
                                }
                            }
                        }
                    }
                    Ok(Some(Message::Close(_))) => break,
                    Ok(Some(_)) => {}
                    Ok(None) => break,
                    Err(error) => {
                        tracing::warn!("terminal WS receive error: {}", error);
                        break;
                    }
                }
            }
        }
    }

    let _ = deployment.pty().close_session(session_id).await;
}

async fn send_error(socket: &mut MaybeSignedWebSocket, message: &str) -> anyhow::Result<()> {
    let msg = TerminalMessage::Error {
        message: message.to_string(),
    };
    let json = serde_json::to_string(&msg).unwrap_or_default();
    socket.send(Message::Text(json.into())).await?;
    socket.close().await?;
    Ok(())
}

pub(super) fn router() -> Router<DeploymentImpl> {
    Router::new().route("/terminal/ws", get(terminal_ws))
}
