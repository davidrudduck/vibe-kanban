use std::sync::Arc;

use agent_client_protocol::{self as acp};
use async_trait::async_trait;
use tokio::sync::{Mutex, mpsc};
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};
use workspace_utils::approvals::ApprovalStatus;

use crate::{
    approvals::{ExecutorApprovalError, ExecutorApprovalService},
    executors::acp::{AcpEvent, ApprovalResponse},
};

/// ACP client that handles agent-client protocol communication
#[derive(Clone)]
pub struct AcpClient {
    event_tx: mpsc::Sender<AcpEvent>,
    approvals: Option<Arc<dyn ExecutorApprovalService>>,
    feedback_queue: Arc<Mutex<Vec<String>>>,
    cancel: CancellationToken,
}

impl AcpClient {
    /// Create a new ACP client
    pub fn new(
        event_tx: mpsc::Sender<AcpEvent>,
        approvals: Option<Arc<dyn ExecutorApprovalService>>,
        cancel: CancellationToken,
    ) -> Self {
        Self {
            event_tx,
            approvals,
            feedback_queue: Arc::new(Mutex::new(Vec::new())),
            cancel,
        }
    }

    pub fn record_user_prompt_event(&self, prompt: &str) {
        // User prompts are transcript-class and high-volume; dropping under
        // sustained backpressure is acceptable.
        self.send_transcript_event(AcpEvent::User(prompt.to_string()));
    }

    /// Send a transcript-class event (e.g. model text chunks, tool updates).
    /// Uses `try_send` so producers are never blocked by a slow forwarder;
    /// the event is dropped with a `warn!` if the channel is full.
    fn send_transcript_event(&self, event: AcpEvent) {
        match self.event_tx.try_send(event) {
            Ok(()) => {}
            Err(mpsc::error::TrySendError::Full(_)) => {
                warn!("ACP event channel full; dropping transcript event");
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                // Receiver dropped — happens during shutdown. Quiet.
            }
        }
    }

    /// Send a control-class event (approval signals, session start/done,
    /// errors). These drive cancellation, completion, and approval flow; a
    /// dropped control event leaves the session in a stuck/wrong state. Uses
    /// `send().await` to apply backpressure rather than dropping on full.
    async fn send_control_event(&self, event: AcpEvent) {
        if let Err(e) = self.event_tx.send(event).await {
            warn!(
                "Failed to send ACP control event (receiver closed): {}",
                e
            );
        }
    }

    /// Queue a user feedback message to be sent after a denial.
    pub async fn enqueue_feedback(&self, message: String) {
        let trimmed = message.trim().to_string();
        if !trimmed.is_empty() {
            let mut q = self.feedback_queue.lock().await;
            q.push(trimmed);
        }
    }

    /// Drain and return queued feedback messages.
    pub async fn drain_feedback(&self) -> Vec<String> {
        let mut q = self.feedback_queue.lock().await;
        q.drain(..).collect()
    }
}

#[async_trait(?Send)]
impl acp::Client for AcpClient {
    async fn request_permission(
        &self,
        args: acp::RequestPermissionRequest,
    ) -> Result<acp::RequestPermissionResponse, acp::Error> {
        self.send_control_event(AcpEvent::RequestPermission(args.clone()))
            .await;

        if self.approvals.is_none() {
            // Auto-approve with best available option when no approval service is configured
            let chosen_option = args
                .options
                .iter()
                .find(|o| matches!(o.kind, acp::PermissionOptionKind::AllowAlways))
                .or_else(|| {
                    args.options
                        .iter()
                        .find(|o| matches!(o.kind, acp::PermissionOptionKind::AllowOnce))
                })
                .or_else(|| args.options.first());

            let outcome = if let Some(opt) = chosen_option {
                debug!("Auto-approving permission with option: {}", opt.option_id);
                acp::RequestPermissionOutcome::Selected(acp::SelectedPermissionOutcome::new(
                    opt.option_id.clone(),
                ))
            } else {
                warn!("No permission options available, cancelling");
                acp::RequestPermissionOutcome::Cancelled
            };

            return Ok(acp::RequestPermissionResponse::new(outcome));
        }

        let tool_call_id = args.tool_call.tool_call_id.0.to_string();
        let tool_name = args.tool_call.fields.title.as_deref().unwrap_or("tool");
        let approval_service = self
            .approvals
            .as_ref()
            .ok_or(ExecutorApprovalError::ServiceUnavailable)
            .map_err(|_| acp::Error::invalid_request())?;

        let approval_id = match approval_service.create_tool_approval(tool_name).await {
            Ok(id) => id,
            Err(err) => return self.handle_approval_error(err, &tool_call_id).await,
        };

        self.send_control_event(AcpEvent::ApprovalRequested {
            tool_call_id: tool_call_id.clone(),
            approval_id: approval_id.clone(),
        })
        .await;

        let status = match approval_service
            .wait_tool_approval(&approval_id, self.cancel.clone())
            .await
        {
            Ok(s) => s,
            Err(err) => return self.handle_approval_error(err, &tool_call_id).await,
        };

        // Map our ApprovalStatus to ACP outcome
        let outcome = match &status {
            ApprovalStatus::Approved => {
                let chosen = args
                    .options
                    .iter()
                    .find(|o| matches!(o.kind, acp::PermissionOptionKind::AllowOnce));
                if let Some(opt) = chosen {
                    acp::RequestPermissionOutcome::Selected(acp::SelectedPermissionOutcome::new(
                        opt.option_id.clone(),
                    ))
                } else {
                    tracing::error!("No suitable approval option found, cancelling");
                    return Err(acp::Error::invalid_request());
                }
            }
            ApprovalStatus::Denied { reason } => {
                // If user provided a reason, queue it to send after denial
                if let Some(feedback) = reason.as_ref() {
                    self.enqueue_feedback(feedback.clone()).await;
                }
                let chosen = args
                    .options
                    .iter()
                    .find(|o| matches!(o.kind, acp::PermissionOptionKind::RejectOnce));
                if let Some(opt) = chosen {
                    acp::RequestPermissionOutcome::Selected(acp::SelectedPermissionOutcome::new(
                        opt.option_id.clone(),
                    ))
                } else {
                    warn!("No permission options for denial, cancelling");
                    acp::RequestPermissionOutcome::Cancelled
                }
            }
            ApprovalStatus::TimedOut => {
                warn!("Approval timed out");
                acp::RequestPermissionOutcome::Cancelled
            }
            ApprovalStatus::Pending => {
                // This should not occur after waiter resolves
                warn!("Approval resolved to Pending");
                acp::RequestPermissionOutcome::Cancelled
            }
        };

        self.send_control_event(AcpEvent::ApprovalResponse(ApprovalResponse {
            tool_call_id: tool_call_id.clone(),
            status: status.clone(),
        }))
        .await;

        Ok(acp::RequestPermissionResponse::new(outcome))
    }

    async fn session_notification(&self, args: acp::SessionNotification) -> Result<(), acp::Error> {
        // Convert to typed events
        let event = match args.update {
            acp::SessionUpdate::AgentMessageChunk(chunk) => Some(AcpEvent::Message(chunk.content)),
            acp::SessionUpdate::AgentThoughtChunk(chunk) => Some(AcpEvent::Thought(chunk.content)),
            acp::SessionUpdate::ToolCall(tc) => Some(AcpEvent::ToolCall(tc)),
            acp::SessionUpdate::ToolCallUpdate(update) => Some(AcpEvent::ToolUpdate(update)),
            acp::SessionUpdate::Plan(plan) => Some(AcpEvent::Plan(plan)),
            _ => Some(AcpEvent::Other(args)),
        };

        if let Some(event) = event {
            // All variants emitted here are transcript-class chunks.
            self.send_transcript_event(event);
        }

        Ok(())
    }

    // File system operations - not implemented as we don't expose FS
    async fn write_text_file(
        &self,
        _args: acp::WriteTextFileRequest,
    ) -> Result<acp::WriteTextFileResponse, acp::Error> {
        Err(acp::Error::method_not_found())
    }

    async fn read_text_file(
        &self,
        _args: acp::ReadTextFileRequest,
    ) -> Result<acp::ReadTextFileResponse, acp::Error> {
        Err(acp::Error::method_not_found())
    }

    // Terminal operations - not implemented
    async fn create_terminal(
        &self,
        _args: acp::CreateTerminalRequest,
    ) -> Result<acp::CreateTerminalResponse, acp::Error> {
        Err(acp::Error::method_not_found())
    }

    async fn terminal_output(
        &self,
        _args: acp::TerminalOutputRequest,
    ) -> Result<acp::TerminalOutputResponse, acp::Error> {
        Err(acp::Error::method_not_found())
    }

    async fn release_terminal(
        &self,
        _args: acp::ReleaseTerminalRequest,
    ) -> Result<acp::ReleaseTerminalResponse, acp::Error> {
        Err(acp::Error::method_not_found())
    }

    async fn wait_for_terminal_exit(
        &self,
        _args: acp::WaitForTerminalExitRequest,
    ) -> Result<acp::WaitForTerminalExitResponse, acp::Error> {
        Err(acp::Error::method_not_found())
    }

    async fn kill_terminal_command(
        &self,
        _args: acp::KillTerminalCommandRequest,
    ) -> Result<acp::KillTerminalCommandResponse, acp::Error> {
        Err(acp::Error::method_not_found())
    }

    // Extension methods
    async fn ext_method(&self, _args: acp::ExtRequest) -> Result<acp::ExtResponse, acp::Error> {
        Err(acp::Error::method_not_found())
    }

    async fn ext_notification(&self, _args: acp::ExtNotification) -> Result<(), acp::Error> {
        Ok(())
    }
}

impl AcpClient {
    async fn handle_approval_error(
        &self,
        err: ExecutorApprovalError,
        tool_call_id: &str,
    ) -> Result<acp::RequestPermissionResponse, acp::Error> {
        if let ExecutorApprovalError::Cancelled = err {
            debug!("ACP approval cancelled for tool_call_id={}", tool_call_id);
            Ok(acp::RequestPermissionResponse::new(
                acp::RequestPermissionOutcome::Cancelled,
            ))
        } else {
            tracing::error!(
                "ACP approval wait failed for tool_call_id={}: {err}",
                tool_call_id
            );
            self.send_control_event(AcpEvent::ApprovalResponse(ApprovalResponse {
                tool_call_id: tool_call_id.to_string(),
                status: ApprovalStatus::TimedOut,
            }))
            .await;
            Err(acp::Error::internal_error())
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[tokio::test]
    async fn transcript_event_drops_when_channel_full_instead_of_blocking() {
        // Build a bounded event channel with capacity 2. Transcript events
        // (User in this case) must drop on full without blocking the producer.
        let (tx, mut rx) = mpsc::channel::<AcpEvent>(2);
        let client = AcpClient::new(tx, None, CancellationToken::new());

        client.record_user_prompt_event("a");
        client.record_user_prompt_event("b");
        // No recv yet; channel is full. Third call must not deadlock the test.
        client.record_user_prompt_event("c");

        let mut count = 0;
        while rx.try_recv().is_ok() {
            count += 1;
        }
        assert_eq!(count, 2, "exactly two transcript events should reach the receiver");
    }

    #[tokio::test]
    async fn control_event_waits_for_capacity_instead_of_dropping() {
        // Capacity 1. Fill it with a transcript event so the channel is full.
        // A control event sent into the full channel must NOT silently drop —
        // it must wait for the receiver to drain.
        let (tx, mut rx) = mpsc::channel::<AcpEvent>(1);
        let client = AcpClient::new(tx, None, CancellationToken::new());
        client.record_user_prompt_event("first");

        // The send_control_event call should not complete within a short
        // window — it's parked waiting for capacity. A bug (try_send-style
        // drop) would make this complete immediately.
        let timed_out = tokio::time::timeout(
            Duration::from_millis(50),
            client.send_control_event(AcpEvent::Done("ok".to_string())),
        )
        .await
        .is_err();
        assert!(timed_out, "send_control_event must block when channel is full");

        // When the timeout fired, the in-flight `send` future inside
        // `send_control_event` was dropped, which cancels the pending send —
        // the Done event never made it into the channel. Drain the User event
        // we put there at the start of the test.
        match rx.try_recv() {
            Ok(AcpEvent::User(s)) => assert_eq!(s, "first"),
            other => panic!("expected User, got {other:?}"),
        }
        // Issue a fresh `send_control_event` on the now-empty channel; this is
        // a new send (not a resumption of the cancelled one) and should
        // complete immediately.
        client.send_control_event(AcpEvent::Done("ok".to_string())).await;
        match rx.try_recv() {
            Ok(AcpEvent::Done(s)) => assert_eq!(s, "ok"),
            other => panic!("expected Done, got {other:?}"),
        }
    }
}
