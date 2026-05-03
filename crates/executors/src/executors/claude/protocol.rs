use std::sync::Arc;

use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{ChildStdin, ChildStdout},
    sync::{Mutex, mpsc},
};
use tokio_util::sync::{CancellationToken, DropGuard};

use super::types::{CLIMessage, ControlRequestType, ControlResponseMessage, ControlResponseType};
use crate::{
    approvals::ExecutorApprovalError,
    executors::{
        ExecutorError,
        claude::{
            client::ClaudeAgentClient,
            types::{Message, PermissionMode, SDKControlRequest, SDKControlRequestType},
        },
    },
};

/// Handles bidirectional control protocol communication.
///
/// `ProtocolPeer` owns the write half (`ChildStdin`) and spawns a background
/// reader task that owns the read half (`ChildStdout`). The reader task is
/// driven by a private `CancellationToken` that is fired automatically when
/// the **last** clone of `ProtocolPeer` is dropped. This guarantees that
/// removing the peer from `LocalContainerService::protocol_peers` causes the
/// reader to exit promptly, releases the `ChildStdin` write end so the child
/// sees EOF, and lets process reaping complete.
///
/// CRITICAL invariant: the reader task itself must NOT hold a clone of the
/// `Arc<DropGuard>` (it only carries a `CancellationToken` clone), otherwise
/// the reader would keep itself alive forever — turning the leak this struct
/// was designed to prevent into a self-referential one.
///
/// TURN-END handling: the Claude Agent SDK CLI keeps reading stdin until it
/// sees EOF, so the *process* would otherwise stay alive after a turn ends
/// even though `CLIMessage::Result` already signaled end-of-turn. To make the
/// UI spinner clear when the agent finishes, the reader fires
/// `turn_idle_tx` on Result and the container drops this peer in response.
/// Dropping releases the last `ChildStdin` clone, the SDK sees EOF, exits,
/// and the OS-exit watcher catches it — running the normal cleanup that
/// updates `execution_process.status` to `'completed'` and broadcasts the
/// patch the frontend needs to clear the spinner.
///
/// Follow-up messages from the user during the brief mid-turn window go
/// through `inject_message`. After the turn ends and the peer is dropped,
/// follow-ups go through `sessionsApi.followUp` (which spawns a new process
/// with `--resume`) — same as before commit b9cfdd27d.
#[derive(Clone)]
pub struct ProtocolPeer {
    stdin: Arc<Mutex<ChildStdin>>,
    /// Wrapped in `Arc` so all clones share. When the last clone of the peer
    /// is dropped (typically when the `LocalContainerService::protocol_peers`
    /// HashMap entry is removed), `DropGuard::drop` cancels `reader_cancel`
    /// and the reader task exits.
    _reader_drop_guard: Arc<DropGuard>,
}

impl ProtocolPeer {
    pub fn spawn(
        stdin: ChildStdin,
        stdout: ChildStdout,
        client: Arc<ClaudeAgentClient>,
        executor_cancel: CancellationToken,
        turn_idle_tx: mpsc::UnboundedSender<()>,
    ) -> Self {
        let stdin = Arc::new(Mutex::new(stdin));
        let reader_cancel = CancellationToken::new();
        let reader_drop_guard = Arc::new(reader_cancel.clone().drop_guard());

        let peer = Self {
            stdin: stdin.clone(),
            _reader_drop_guard: reader_drop_guard,
        };

        // Reader task: owns the stdout pipe + a clone of the stdin Arc (for
        // sending interrupts and control responses). It does NOT hold the
        // `_reader_drop_guard` Arc, so the reader cannot keep itself alive
        // past the last public clone of `ProtocolPeer`.
        let reader_stdin = stdin;
        tokio::spawn(async move {
            if let Err(e) = read_loop(
                reader_stdin,
                stdout,
                client,
                executor_cancel,
                reader_cancel,
                turn_idle_tx,
            )
            .await
            {
                tracing::error!("Protocol reader loop error: {}", e);
            }
        });

        peer
    }

    pub async fn send_user_message(&self, content: String) -> Result<(), ExecutorError> {
        let message = Message::new_user(content);
        write_json(&self.stdin, &message).await
    }

    pub async fn initialize(&self, hooks: Option<serde_json::Value>) -> Result<(), ExecutorError> {
        write_json(
            &self.stdin,
            &SDKControlRequest::new(SDKControlRequestType::Initialize { hooks }),
        )
        .await
    }

    pub async fn interrupt(&self) -> Result<(), ExecutorError> {
        write_json(
            &self.stdin,
            &SDKControlRequest::new(SDKControlRequestType::Interrupt {}),
        )
        .await
    }

    pub async fn set_permission_mode(&self, mode: PermissionMode) -> Result<(), ExecutorError> {
        write_json(
            &self.stdin,
            &SDKControlRequest::new(SDKControlRequestType::SetPermissionMode { mode }),
        )
        .await
    }

    pub async fn send_hook_response(
        &self,
        request_id: String,
        hook_output: serde_json::Value,
    ) -> Result<(), ExecutorError> {
        send_hook_response(&self.stdin, request_id, hook_output).await
    }
}

// ---------------------------------------------------------------------------
// Free helpers used by both the public `ProtocolPeer` API and the background
// reader task. Keeping them as free functions taking `&Arc<Mutex<ChildStdin>>`
// is what allows the reader task to use them WITHOUT holding a clone of
// `ProtocolPeer` (and therefore without holding the drop guard).
// ---------------------------------------------------------------------------

async fn write_json<T: serde::Serialize>(
    stdin: &Mutex<ChildStdin>,
    message: &T,
) -> Result<(), ExecutorError> {
    let json = serde_json::to_string(message)?;
    let mut stdin = stdin.lock().await;
    stdin.write_all(json.as_bytes()).await?;
    stdin.write_all(b"\n").await?;
    stdin.flush().await?;
    Ok(())
}

async fn send_hook_response(
    stdin: &Mutex<ChildStdin>,
    request_id: String,
    hook_output: serde_json::Value,
) -> Result<(), ExecutorError> {
    write_json(
        stdin,
        &ControlResponseMessage::new(ControlResponseType::Success {
            request_id,
            response: Some(hook_output),
        }),
    )
    .await
}

async fn send_error(
    stdin: &Mutex<ChildStdin>,
    request_id: String,
    error: String,
) -> Result<(), ExecutorError> {
    write_json(
        stdin,
        &ControlResponseMessage::new(ControlResponseType::Error {
            request_id,
            error: Some(error),
        }),
    )
    .await
}

async fn send_interrupt(stdin: &Mutex<ChildStdin>) -> Result<(), ExecutorError> {
    write_json(
        stdin,
        &SDKControlRequest::new(SDKControlRequestType::Interrupt {}),
    )
    .await
}

async fn handle_control_request(
    stdin: &Arc<Mutex<ChildStdin>>,
    client: &Arc<ClaudeAgentClient>,
    request_id: String,
    request: ControlRequestType,
) {
    match request {
        ControlRequestType::CanUseTool {
            tool_name,
            input,
            permission_suggestions,
            blocked_paths: _,
            tool_use_id,
        } => match client
            .on_can_use_tool(tool_name, input, permission_suggestions, tool_use_id)
            .await
        {
            Ok(result) => {
                if let Err(e) =
                    send_hook_response(stdin, request_id, serde_json::to_value(result).unwrap())
                        .await
                {
                    tracing::error!("Failed to send permission result: {e}");
                }
            }
            Err(ExecutorError::ExecutorApprovalError(ExecutorApprovalError::Cancelled)) => {}
            Err(e) => {
                tracing::error!("Error in on_can_use_tool: {e}");
                if let Err(e2) = send_error(stdin, request_id, e.to_string()).await {
                    tracing::error!("Failed to send error response: {e2}");
                }
            }
        },
        ControlRequestType::HookCallback {
            callback_id,
            input,
            tool_use_id,
        } => match client
            .on_hook_callback(callback_id, input, tool_use_id)
            .await
        {
            Ok(hook_output) => {
                if let Err(e) = send_hook_response(stdin, request_id, hook_output).await {
                    tracing::error!("Failed to send hook callback result: {e}");
                }
            }
            Err(e) => {
                tracing::error!("Error in on_hook_callback: {e}");
                if let Err(e2) = send_error(stdin, request_id, e.to_string()).await {
                    tracing::error!("Failed to send error response: {e2}");
                }
            }
        },
    }
}

async fn read_loop(
    stdin: Arc<Mutex<ChildStdin>>,
    stdout: ChildStdout,
    client: Arc<ClaudeAgentClient>,
    executor_cancel: CancellationToken,
    reader_cancel: CancellationToken,
    turn_idle_tx: mpsc::UnboundedSender<()>,
) -> Result<(), ExecutorError> {
    let mut reader = BufReader::new(stdout);
    let mut buffer = String::new();
    let mut interrupt_sent = false;

    loop {
        buffer.clear();
        tokio::select! {
            biased;
            // Last public clone of ProtocolPeer was dropped — exit immediately
            // so the ChildStdin Arc held by this task is released and the child
            // sees EOF on its stdin.
            _ = reader_cancel.cancelled() => {
                tracing::debug!("Protocol reader exiting: peer dropped");
                break;
            }
            _ = executor_cancel.cancelled(), if !interrupt_sent => {
                interrupt_sent = true;
                tracing::info!("Cancellation received in read_loop, sending interrupt to Claude");
                if let Err(e) = send_interrupt(&stdin).await {
                    tracing::warn!("Failed to send interrupt to Claude: {e}");
                }
                // Continue the loop to read Claude's response (it should send a result)
            }
            line_result = reader.read_line(&mut buffer) => {
                match line_result {
                    Ok(0) => break, // EOF
                    Ok(_) => {
                        let line = buffer.trim();
                        if line.is_empty() {
                            continue;
                        }
                        client.log_message(line).await?;

                        // Parse and handle control messages
                        match serde_json::from_str::<CLIMessage>(line) {
                            Ok(CLIMessage::ControlRequest {
                                request_id,
                                request,
                            }) => {
                                handle_control_request(&stdin, &client, request_id, request).await;
                            }
                            Ok(CLIMessage::Result(_)) => {
                                // End-of-turn, NOT end-of-execution. The SDK
                                // stays alive on stdin so a follow-up turn can
                                // be injected via ProtocolPeer. We break out
                                // of the read so the per-turn select can
                                // re-arm; the process keeps running until the
                                // OS-exit watcher catches it.
                                //
                                // Notify the container that this turn is
                                // idle so the UI's spinner can clear even
                                // though the underlying process keeps
                                // running. Send is best-effort: if the
                                // container has already torn down its
                                // listener, the receiver will be dropped
                                // and this errors silently — acceptable.
                                let _ = turn_idle_tx.send(());
                                break;
                            }
                            _ => {}
                        }
                    }
                    Err(e) => {
                        tracing::error!("Error reading stdout: {}", e);
                        break;
                    }
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::time::timeout;

    use super::*;

    /// When the last clone of `ProtocolPeer` is dropped, the reader task's
    /// `reader_cancel` token must fire so the task exits promptly.
    /// This is the invariant that prevents the lifecycle leak that caused
    /// the "executor end no longer detected" regression.
    #[tokio::test]
    async fn last_peer_clone_drop_cancels_reader() {
        // Create a token we can observe from the test, then build an Arc<DropGuard>
        // exactly the way ProtocolPeer::spawn does it.
        let reader_cancel = CancellationToken::new();
        let observable = reader_cancel.clone();
        let drop_guard = Arc::new(reader_cancel.clone().drop_guard());

        // Peer-shaped struct: two "clones" share the guard.
        let peer_a = drop_guard.clone();
        let peer_b = drop_guard;

        // Drop one clone: the other still holds the guard, so the token must
        // remain un-cancelled.
        drop(peer_a);
        assert!(
            !observable.is_cancelled(),
            "reader cancel must not fire while another peer clone exists"
        );

        // Drop the last clone: the underlying DropGuard runs, cancelling.
        drop(peer_b);
        assert!(
            observable.is_cancelled(),
            "reader cancel must fire when the last peer clone is dropped"
        );

        // And `cancelled().await` must resolve immediately — what the reader
        // loop relies on to exit.
        timeout(Duration::from_millis(100), observable.cancelled())
            .await
            .expect("reader_cancel.cancelled() must resolve after last peer drop");
    }
}
