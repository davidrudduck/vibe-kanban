use std::fmt;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio_util::sync::CancellationToken;
use workspace_utils::approvals::{ApprovalStatus, QuestionStatus};

use crate::logs::AskUserQuestionItem;

/// Errors emitted by executor approval services.
#[derive(Debug, Error)]
pub enum ExecutorApprovalError {
    #[error("executor approval session not registered")]
    SessionNotRegistered,
    #[error("executor approval request failed: {0}")]
    RequestFailed(String),
    #[error("executor approval service unavailable")]
    ServiceUnavailable,
    #[error("executor approval request cancelled")]
    Cancelled,
}

impl ExecutorApprovalError {
    pub fn request_failed<E: fmt::Display>(err: E) -> Self {
        Self::RequestFailed(err.to_string())
    }
}

/// Abstraction for executor approval backends.
#[async_trait]
pub trait ExecutorApprovalService: Send + Sync {
    /// Creates a tool approval request. Returns the approval_id immediately.
    async fn create_tool_approval(&self, tool_name: &str) -> Result<String, ExecutorApprovalError>;

    /// Creates a question approval request. Returns the approval_id immediately.
    async fn create_question_approval(
        &self,
        tool_name: &str,
        questions: &[AskUserQuestionItem],
    ) -> Result<String, ExecutorApprovalError>;

    /// Waits for a tool approval to be resolved. Blocks until approved/denied/timed out.
    async fn wait_tool_approval(
        &self,
        approval_id: &str,
        cancel: CancellationToken,
    ) -> Result<ApprovalStatus, ExecutorApprovalError>;

    /// Waits for a question to be answered. Blocks until answered/timed out.
    async fn wait_question_answer(
        &self,
        approval_id: &str,
        cancel: CancellationToken,
    ) -> Result<QuestionStatus, ExecutorApprovalError>;
}

#[derive(Debug, Default)]
pub struct NoopExecutorApprovalService;

#[async_trait]
impl ExecutorApprovalService for NoopExecutorApprovalService {
    async fn create_tool_approval(
        &self,
        _tool_name: &str,
    ) -> Result<String, ExecutorApprovalError> {
        Ok("noop".to_string())
    }

    async fn create_question_approval(
        &self,
        _tool_name: &str,
        _questions: &[AskUserQuestionItem],
    ) -> Result<String, ExecutorApprovalError> {
        Ok("noop".to_string())
    }

    async fn wait_tool_approval(
        &self,
        _approval_id: &str,
        _cancel: CancellationToken,
    ) -> Result<ApprovalStatus, ExecutorApprovalError> {
        Ok(ApprovalStatus::Approved)
    }

    async fn wait_question_answer(
        &self,
        _approval_id: &str,
        _cancel: CancellationToken,
    ) -> Result<QuestionStatus, ExecutorApprovalError> {
        Err(ExecutorApprovalError::ServiceUnavailable)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolCallMetadata {
    pub tool_call_id: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logs::{AskUserQuestionItem, AskUserQuestionOption};

    #[tokio::test]
    async fn noop_service_accepts_structured_questions() {
        let svc = NoopExecutorApprovalService;
        let questions = vec![AskUserQuestionItem {
            question: "Pick one".to_string(),
            header: "pick".to_string(),
            options: vec![AskUserQuestionOption {
                label: "A".to_string(),
                description: "Option A".to_string(),
            }],
            multi_select: false,
        }];
        let result = svc
            .create_question_approval("AskUserQuestion", &questions)
            .await;
        assert!(result.is_ok());
    }
}
