use std::sync::Arc;

use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{ChildStdin, ChildStdout},
    sync::Mutex,
};

use super::types::{
    CLIMessage, ControlRequestType, ControlResponseMessage, ControlResponseType,
    SDKControlRequestMessage,
};
use crate::executors::{
    ExecutorError,
    claude::{
        client::ClaudeAgentClient,
        types::{PermissionMode, SDKControlRequestType},
    },
};
use workspace_utils::approvals::Question;

/// Handles bidirectional control protocol communication.
/// Allows sending messages to a running Claude Code process via stdin.
#[derive(Clone, Debug)]
pub struct ProtocolPeer {
    stdin: Arc<Mutex<ChildStdin>>,
}

impl ProtocolPeer {
    pub fn spawn(stdin: ChildStdin, stdout: ChildStdout, client: Arc<ClaudeAgentClient>) -> Self {
        let peer = Self {
            stdin: Arc::new(Mutex::new(stdin)),
        };

        let reader_peer = peer.clone();
        tokio::spawn(async move {
            if let Err(e) = reader_peer.read_loop(stdout, client).await {
                tracing::error!("Protocol reader loop error: {}", e);
            }
        });

        peer
    }

    async fn read_loop(
        &self,
        stdout: ChildStdout,
        client: Arc<ClaudeAgentClient>,
    ) -> Result<(), ExecutorError> {
        let mut reader = BufReader::new(stdout);
        let mut buffer = String::new();

        loop {
            buffer.clear();
            match reader.read_line(&mut buffer).await {
                Ok(0) => break, // EOF
                Ok(_) => {
                    let line = buffer.trim();
                    if line.is_empty() {
                        continue;
                    }
                    // Parse message using typed enum
                    match serde_json::from_str::<CLIMessage>(line) {
                        Ok(CLIMessage::ControlRequest {
                            request_id,
                            request,
                        }) => {
                            self.handle_control_request(&client, request_id, request)
                                .await;
                        }
                        Ok(CLIMessage::ControlResponse { .. }) => {}
                        Ok(CLIMessage::Result(value)) => {
                            // Check if result string is empty - empty results may be
                            // intermediate signals (context summarization, state transitions)
                            // rather than true session completion
                            let result_str = value
                                .get("result")
                                .and_then(|r| r.as_str())
                                .unwrap_or("");
                            let has_result = !result_str.is_empty();

                            tracing::info!(
                                result_value = ?value,
                                has_result = has_result,
                                "Claude protocol: Received Result message"
                            );
                            client.on_non_control(line).await?;

                            // Only break if result has content - empty results may be intermediate
                            if has_result {
                                tracing::info!("Result has content, breaking read loop");
                                break;
                            } else {
                                tracing::warn!(
                                    "Ignoring Result message with empty result string - \
                                     may be intermediate context switch, continuing read loop"
                                );
                            }
                        }
                        _ => {
                            client.on_non_control(line).await?;
                        }
                    }
                }
                Err(e) => {
                    tracing::error!("Error reading stdout: {}", e);
                    break;
                }
            }
        }
        Ok(())
    }

    async fn handle_control_request(
        &self,
        client: &Arc<ClaudeAgentClient>,
        request_id: String,
        request: ControlRequestType,
    ) {
        match request {
            ControlRequestType::CanUseTool {
                tool_name,
                input,
                permission_suggestions,
                tool_use_id,
            } => {
                // Special case: AskUserQuestion arriving via CanUseTool needs PermissionResult format
                // Claude SDK expects { "behavior": "allow", "updatedInput": { "questions": [...], "answers": {...} } }
                // See: https://platform.claude.com/docs/en/agent-sdk/permissions
                if tool_name == "AskUserQuestion" {
                    // Extract questions from input and use the existing AskUserQuestion handler
                    let questions_result: Result<Vec<Question>, _> = input
                        .get("questions")
                        .map(|q| serde_json::from_value(q.clone()))
                        .unwrap_or(Ok(vec![]));

                    match questions_result {
                        Ok(questions) if !questions.is_empty() => {
                            // Get user answers via the question handler
                            match client.on_ask_user_question(questions, tool_use_id).await {
                                Ok(result) => {
                                    // Wrap response in PermissionResult format for CanUseTool
                                    let permission_response = if let Some(answers) = result.get("answers") {
                                        // Build updatedInput with original questions + user answers
                                        let mut updated_input = input.clone();
                                        updated_input["answers"] = answers.clone();
                                        serde_json::json!({
                                            "behavior": "allow",
                                            "updatedInput": updated_input
                                        })
                                    } else {
                                        // Error case - deny with message
                                        let error_msg = result.get("error")
                                            .and_then(|e| e.as_str())
                                            .unwrap_or("Unknown error");
                                        serde_json::json!({
                                            "behavior": "deny",
                                            "message": error_msg
                                        })
                                    };
                                    if let Err(e) =
                                        self.send_hook_response(request_id, permission_response).await
                                    {
                                        tracing::error!("Failed to send question answer: {e}");
                                    }
                                }
                                Err(e) => {
                                    tracing::error!("Error in on_ask_user_question: {e}");
                                    if let Err(e2) =
                                        self.send_error(request_id, e.to_string()).await
                                    {
                                        tracing::error!("Failed to send error response: {e2}");
                                    }
                                }
                            }
                            return;
                        }
                        _ => {
                            tracing::warn!(
                                "AskUserQuestion via CanUseTool missing or invalid questions"
                            );
                            // Fall through to normal CanUseTool handling which will deny
                        }
                    }
                }

                // Normal CanUseTool handling (non-AskUserQuestion or fallback)
                match client
                    .on_can_use_tool(tool_name, input, permission_suggestions, tool_use_id)
                    .await
                {
                    Ok(result) => {
                        if let Err(e) = self
                            .send_hook_response(request_id, serde_json::to_value(result).unwrap())
                            .await
                        {
                            tracing::error!("Failed to send permission result: {e}");
                        }
                    }
                    Err(e) => {
                        tracing::error!("Error in on_can_use_tool: {e}");
                        if let Err(e2) = self.send_error(request_id, e.to_string()).await {
                            tracing::error!("Failed to send error response: {e2}");
                        }
                    }
                }
            }
            ControlRequestType::HookCallback {
                callback_id,
                input,
                tool_use_id,
            } => {
                match client
                    .on_hook_callback(callback_id, input, tool_use_id)
                    .await
                {
                    Ok(hook_output) => {
                        if let Err(e) = self.send_hook_response(request_id, hook_output).await {
                            tracing::error!("Failed to send hook callback result: {e}");
                        }
                    }
                    Err(e) => {
                        tracing::error!("Error in on_hook_callback: {e}");
                        if let Err(e2) = self.send_error(request_id, e.to_string()).await {
                            tracing::error!("Failed to send error response: {e2}");
                        }
                    }
                }
            }
            ControlRequestType::AskUserQuestion {
                questions,
                tool_use_id,
            } => match client.on_ask_user_question(questions, tool_use_id).await {
                Ok(result) => {
                    if let Err(e) = self.send_hook_response(request_id, result).await {
                        tracing::error!("Failed to send question answer: {e}");
                    }
                }
                Err(e) => {
                    tracing::error!("Error in on_ask_user_question: {e}");
                    if let Err(e2) = self.send_error(request_id, e.to_string()).await {
                        tracing::error!("Failed to send error response: {e2}");
                    }
                }
            },
        }
    }

    pub async fn send_hook_response(
        &self,
        request_id: String,
        hook_output: serde_json::Value,
    ) -> Result<(), ExecutorError> {
        self.send_json(&ControlResponseMessage::new(ControlResponseType::Success {
            request_id,
            response: Some(hook_output),
        }))
        .await
    }

    /// Send error response to CLI
    async fn send_error(&self, request_id: String, error: String) -> Result<(), ExecutorError> {
        self.send_json(&ControlResponseMessage::new(ControlResponseType::Error {
            request_id,
            error: Some(error),
        }))
        .await
    }

    /// Send JSON message to stdin
    async fn send_json<T: serde::Serialize>(&self, message: &T) -> Result<(), ExecutorError> {
        let json = serde_json::to_string(message)?;
        let mut stdin = self.stdin.lock().await;
        stdin.write_all(json.as_bytes()).await?;
        stdin.write_all(b"\n").await?;
        stdin.flush().await?;
        Ok(())
    }

    pub async fn send_user_message(&self, content: String) -> Result<(), ExecutorError> {
        let message = serde_json::json!({
            "type": "user",
            "message": {
                "role": "user",
                "content": content
            }
        });
        self.send_json(&message).await
    }

    pub async fn initialize(&self, hooks: Option<serde_json::Value>) -> Result<(), ExecutorError> {
        self.send_json(&SDKControlRequestMessage::new(
            SDKControlRequestType::Initialize { hooks },
        ))
        .await
    }

    pub async fn set_permission_mode(&self, mode: PermissionMode) -> Result<(), ExecutorError> {
        self.send_json(&SDKControlRequestMessage::new(
            SDKControlRequestType::SetPermissionMode { mode },
        ))
        .await
    }
}
