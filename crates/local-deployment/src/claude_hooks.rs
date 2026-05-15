use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::LocalDeployment;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeHookPayload {
    pub session_id: String,
    #[serde(default)]
    pub transcript_path: Option<String>,
    #[serde(default)]
    pub cwd: Option<String>,
    pub hook_event_name: String,
    #[serde(default)]
    pub tool_name: Option<String>,
    #[serde(default)]
    pub tool_input: Option<serde_json::Value>,
    #[serde(default)]
    pub tool_response: Option<serde_json::Value>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone)]
pub struct ClaudeHookEvent {
    pub execution_id: Uuid,
    pub payload: ClaudeHookPayload,
}

impl LocalDeployment {
    pub async fn handle_claude_hook(
        &self,
        execution_id: Uuid,
        event: String,
        payload: ClaudeHookPayload,
    ) -> anyhow::Result<()> {
        if event_to_hook_event_name(&event) != payload.hook_event_name {
            anyhow::bail!(
                "hook event mismatch: route event '{}' does not match payload hook_event_name '{}'",
                event,
                payload.hook_event_name
            );
        }

        tracing::debug!(
            execution_id = %execution_id,
            event = %event,
            claude_session_id = %payload.session_id,
            "received claude terminal hook"
        );
        Ok(())
    }
}

pub fn event_to_hook_event_name(event: &str) -> String {
    event
        .split('-')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => {
                    let mut segment = first.to_uppercase().collect::<String>();
                    segment.push_str(chars.as_str());
                    segment
                }
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join("")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserializes_common_hook_fields() {
        let payload: ClaudeHookPayload = serde_json::from_value(serde_json::json!({
            "session_id": "claude-session-123",
            "transcript_path": "/tmp/claude.jsonl",
            "cwd": "/tmp/worktree",
            "hook_event_name": "Stop"
        }))
        .unwrap();

        assert_eq!(payload.session_id, "claude-session-123");
        assert_eq!(
            payload.transcript_path.as_deref(),
            Some("/tmp/claude.jsonl")
        );
        assert_eq!(payload.cwd.as_deref(), Some("/tmp/worktree"));
        assert_eq!(payload.hook_event_name, "Stop");
    }

    #[test]
    fn preserves_event_specific_fields() {
        let payload: ClaudeHookPayload = serde_json::from_value(serde_json::json!({
            "session_id": "claude-session-123",
            "hook_event_name": "UserPromptSubmit",
            "prompt": "fix this bug"
        }))
        .unwrap();

        assert_eq!(
            payload.extra.get("prompt").and_then(|value| value.as_str()),
            Some("fix this bug")
        );
    }

    #[test]
    fn validates_route_event_name_against_payload_event_name() {
        assert_eq!(event_to_hook_event_name("stop"), "Stop");
        assert_eq!(event_to_hook_event_name("post-tool-use"), "PostToolUse");
        assert_eq!(
            event_to_hook_event_name("post-tool-use-failure"),
            "PostToolUseFailure"
        );
        assert_eq!(event_to_hook_event_name("session-end"), "SessionEnd");
    }
}
