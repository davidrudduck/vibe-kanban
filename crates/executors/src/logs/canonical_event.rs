use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use ts_rs::TS;
use uuid::Uuid;
use workspace_utils::log_msg::LogMsg;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum CanonicalLogEventType {
    ExecutionStarted,
    ExecutionFinished,
    UserMessage,
    AssistantMessageDelta,
    AssistantMessageFinal,
    ToolStarted,
    ToolDelta,
    ToolFinished,
    SystemStatus,
    RawStdout,
    RawStderr,
    JsonPatch,
    ResetIgnored,
    RefreshRequired,
}

impl CanonicalLogEventType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ExecutionStarted => "execution_started",
            Self::ExecutionFinished => "execution_finished",
            Self::UserMessage => "user_message",
            Self::AssistantMessageDelta => "assistant_message_delta",
            Self::AssistantMessageFinal => "assistant_message_final",
            Self::ToolStarted => "tool_started",
            Self::ToolDelta => "tool_delta",
            Self::ToolFinished => "tool_finished",
            Self::SystemStatus => "system_status",
            Self::RawStdout => "raw_stdout",
            Self::RawStderr => "raw_stderr",
            Self::JsonPatch => "json_patch",
            Self::ResetIgnored => "reset_ignored",
            Self::RefreshRequired => "refresh_required",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CanonicalLogEvent {
    #[ts(type = "string")]
    pub execution_id: Uuid,
    pub source: String,
    pub source_event_id: Option<String>,
    pub event_type: CanonicalLogEventType,
    pub payload_json: Value,
}

impl CanonicalLogEvent {
    pub fn from_log_msg(
        execution_id: Uuid,
        source: impl Into<String>,
        source_event_id: Option<String>,
        sequence: u64,
        msg: &LogMsg,
    ) -> Option<Self> {
        let source = source.into();
        let (event_type, payload_json) = match msg {
            LogMsg::Stdout(content) => {
                text_event_payload(CanonicalLogEventType::RawStdout, content)
            }
            LogMsg::Stderr(content) => {
                text_event_payload(CanonicalLogEventType::RawStderr, content)
            }
            LogMsg::JsonPatch(patch) => (
                CanonicalLogEventType::JsonPatch,
                serde_json::to_value(patch).unwrap_or_else(|_| json!([])),
            ),
            LogMsg::SessionId(session_id) => (
                CanonicalLogEventType::SystemStatus,
                json!({
                    "kind": "session_id",
                    "session_id": session_id,
                }),
            ),
            LogMsg::MessageId(message_id) => (
                CanonicalLogEventType::SystemStatus,
                json!({
                    "kind": "message_id",
                    "message_id": message_id,
                }),
            ),
            LogMsg::Ready => return None,
            LogMsg::Finished => (
                CanonicalLogEventType::ExecutionFinished,
                json!({
                    "terminal": true,
                }),
            ),
        };

        let source_event_id = source_event_id.or_else(|| {
            Some(deterministic_source_event_id(
                execution_id,
                &source,
                sequence,
                event_type,
                &payload_json,
            ))
        });

        Some(Self {
            execution_id,
            source,
            source_event_id,
            event_type,
            payload_json,
        })
    }
}

fn text_event_payload(
    event_type: CanonicalLogEventType,
    content: &str,
) -> (CanonicalLogEventType, Value) {
    let stripped = strip_ansi_escapes::strip_str(content);
    if contains_terminal_reset(content) {
        (
            CanonicalLogEventType::ResetIgnored,
            json!({
                "channel": event_type.as_str(),
                "text": stripped,
                "ignored_reset": true,
            }),
        )
    } else {
        (
            event_type,
            json!({
                "text": stripped,
            }),
        )
    }
}

fn contains_terminal_reset(content: &str) -> bool {
    const RESET_SEQUENCES: [&str; 8] = [
        "\u{1b}c",
        "\u{1b}[2J",
        "\u{1b}[3J",
        "\u{1b}[H",
        "\u{1b}[1;1H",
        "\u{1b}[?1049h",
        "\u{1b}[?1049l",
        "\u{1b}[?47h",
    ];
    RESET_SEQUENCES.iter().any(|seq| content.contains(seq))
}

fn deterministic_source_event_id(
    execution_id: Uuid,
    source: &str,
    sequence: u64,
    event_type: CanonicalLogEventType,
    payload_json: &Value,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(execution_id.as_bytes());
    hasher.update(source.as_bytes());
    hasher.update(sequence.to_be_bytes());
    hasher.update(event_type.as_str().as_bytes());
    hasher.update(
        serde_json::to_string(payload_json)
            .unwrap_or_else(|_| "null".to_string())
            .as_bytes(),
    );
    let digest = hasher.finalize();
    let mut encoded = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(&mut encoded, "{byte:02x}");
    }
    encoded
}

#[cfg(test)]
mod tests {
    use json_patch::Patch;
    use serde_json::json;
    use uuid::Uuid;
    use workspace_utils::log_msg::LogMsg;

    use super::*;

    #[test]
    fn converts_json_patch_to_canonical_event() {
        let execution_id = Uuid::new_v4();
        let patch: Patch = serde_json::from_value(json!([
            {
                "op": "add",
                "path": "/entries/0",
                "value": {
                    "type": "STDOUT",
                    "content": "hello"
                }
            }
        ]))
        .unwrap();

        let event = CanonicalLogEvent::from_log_msg(
            execution_id,
            "claude",
            Some("msg-1".to_string()),
            7,
            &LogMsg::JsonPatch(patch.clone()),
        )
        .unwrap();

        assert_eq!(event.execution_id, execution_id);
        assert_eq!(event.source, "claude");
        assert_eq!(event.source_event_id, Some("msg-1".to_string()));
        assert_eq!(event.event_type, CanonicalLogEventType::JsonPatch);
        assert_eq!(event.payload_json, serde_json::to_value(&patch).unwrap());
    }

    #[test]
    fn strips_clear_sequences_without_clearing_transcript() {
        let event = CanonicalLogEvent::from_log_msg(
            Uuid::new_v4(),
            "claude",
            None,
            2,
            &LogMsg::Stdout("\u{1b}[2J\u{1b}[Hafter clear".to_string()),
        )
        .unwrap();

        assert_eq!(event.event_type, CanonicalLogEventType::ResetIgnored);
        assert_eq!(event.payload_json["text"], "after clear");
        assert_eq!(event.payload_json["ignored_reset"], true);
    }

    #[test]
    fn deterministic_source_id_falls_back_to_sequence() {
        let execution_id = Uuid::new_v4();
        let first = CanonicalLogEvent::from_log_msg(
            execution_id,
            "codex",
            None,
            3,
            &LogMsg::Stdout("hello".to_string()),
        )
        .unwrap();
        let second = CanonicalLogEvent::from_log_msg(
            execution_id,
            "codex",
            None,
            3,
            &LogMsg::Stdout("hello".to_string()),
        )
        .unwrap();

        assert_eq!(first.source_event_id, second.source_event_id);
        assert_eq!(first.event_type, CanonicalLogEventType::RawStdout);
    }

    #[test]
    fn finished_event_has_terminal_payload() {
        let event =
            CanonicalLogEvent::from_log_msg(Uuid::new_v4(), "opencode", None, 9, &LogMsg::Finished)
                .unwrap();

        assert_eq!(event.event_type, CanonicalLogEventType::ExecutionFinished);
        assert_eq!(event.payload_json["terminal"], true);
    }

    #[test]
    fn ready_events_are_not_persisted() {
        assert!(
            CanonicalLogEvent::from_log_msg(Uuid::new_v4(), "claude", None, 1, &LogMsg::Ready,)
                .is_none()
        );
    }
}
