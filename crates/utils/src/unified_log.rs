//! Unified log entry schema for both local and remote log access.
//!
//! This module provides a common data model that works for both local (SQLite/JSONL)
//! and remote (PostgreSQL/Hive) log sources, enabling unified API access with pagination.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use ts_rs::TS;
use uuid::Uuid;

use crate::log_msg::LogMsg;

/// Error type for unified log operations.
#[derive(Debug, Error)]
pub enum UnifiedLogError {
    #[error("failed to parse JSONL: {0}")]
    JsonParse(#[from] serde_json::Error),

    #[error("invalid log format: {0}")]
    InvalidFormat(String),
}

/// Output type for log entries, unified across local and remote sources.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum OutputType {
    Stdout,
    Stderr,
    System,
    JsonPatch,
    SessionId,
    Finished,
    RefreshRequired,
}

impl OutputType {
    /// Convert from the remote database string representation.
    pub fn from_remote_str(s: &str) -> Self {
        match s {
            "stdout" => Self::Stdout,
            "stderr" => Self::Stderr,
            "system" => Self::System,
            "json_patch" => Self::JsonPatch,
            "session_id" => Self::SessionId,
            "finished" => Self::Finished,
            "refresh_required" => Self::RefreshRequired,
            _ => Self::System, // Default fallback for unknown types
        }
    }

    /// Convert to the string representation used in remote database.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Stdout => "stdout",
            Self::Stderr => "stderr",
            Self::System => "system",
            Self::JsonPatch => "json_patch",
            Self::SessionId => "session_id",
            Self::Finished => "finished",
            Self::RefreshRequired => "refresh_required",
        }
    }
}

/// A unified log entry that works for both local and remote log sources.
///
/// This struct provides a consistent interface for log entries regardless of
/// whether they come from local SQLite storage or remote PostgreSQL via Hive.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct LogEntry {
    /// Sequential ID for cursor-based pagination.
    pub id: i64,

    /// The content of the log entry.
    pub content: String,

    /// The type of output (stdout, stderr, system, json_patch, etc.).
    pub output_type: OutputType,

    /// When the log entry was created.
    #[ts(type = "string")]
    pub timestamp: DateTime<Utc>,

    /// The execution ID this log belongs to.
    #[ts(type = "string")]
    pub execution_id: Uuid,
}

impl LogEntry {
    /// Create a new LogEntry.
    pub fn new(
        id: i64,
        content: String,
        output_type: OutputType,
        timestamp: DateTime<Utc>,
        execution_id: Uuid,
    ) -> Self {
        Self {
            id,
            content,
            output_type,
            timestamp,
            execution_id,
        }
    }

    /// Parse a LogEntry from local JSONL format (LogMsg serialization).
    ///
    /// Local logs are stored as JSONL where each line is a serialized LogMsg enum.
    /// This method converts from that format to the unified LogEntry.
    ///
    /// # Arguments
    /// * `line` - A single line of JSONL (serialized LogMsg)
    /// * `id` - The sequential ID to assign to this entry
    /// * `execution_id` - The execution this log belongs to
    /// * `timestamp` - The timestamp for this entry (from the log batch)
    pub fn from_local_jsonl(
        line: &str,
        id: i64,
        execution_id: Uuid,
        timestamp: DateTime<Utc>,
    ) -> Result<Self, UnifiedLogError> {
        let log_msg: LogMsg = serde_json::from_str(line)?;

        let (output_type, content) = match log_msg {
            LogMsg::Stdout(s) => (OutputType::Stdout, s),
            LogMsg::Stderr(s) => (OutputType::Stderr, s),
            LogMsg::JsonPatch(patch) => {
                let content = serde_json::to_string(&patch)
                    .unwrap_or_else(|_| "[]".to_string());
                (OutputType::JsonPatch, content)
            }
            LogMsg::SessionId(s) => (OutputType::SessionId, s),
            LogMsg::Finished => (OutputType::Finished, String::new()),
            LogMsg::RefreshRequired { reason } => (OutputType::RefreshRequired, reason),
        };

        Ok(Self {
            id,
            content,
            output_type,
            timestamp,
            execution_id,
        })
    }

    /// Convert this LogEntry back to a LogMsg for compatibility with existing code.
    ///
    /// Note: This conversion may lose some information (id, timestamp, execution_id).
    pub fn to_log_msg(&self) -> Result<LogMsg, UnifiedLogError> {
        match self.output_type {
            OutputType::Stdout => Ok(LogMsg::Stdout(self.content.clone())),
            OutputType::Stderr => Ok(LogMsg::Stderr(self.content.clone())),
            OutputType::JsonPatch => {
                let patch = serde_json::from_str(&self.content)?;
                Ok(LogMsg::JsonPatch(patch))
            }
            OutputType::SessionId => Ok(LogMsg::SessionId(self.content.clone())),
            OutputType::Finished => Ok(LogMsg::Finished),
            OutputType::RefreshRequired => Ok(LogMsg::RefreshRequired {
                reason: self.content.clone(),
            }),
            OutputType::System => {
                // System messages don't have a direct LogMsg equivalent,
                // treat them as stdout for compatibility
                Ok(LogMsg::Stdout(self.content.clone()))
            }
        }
    }
}

/// Remote log row structure matching the PostgreSQL schema from Hive.
///
/// This is used for converting remote database rows to unified LogEntry.
#[derive(Debug, Clone)]
pub struct RemoteLogRow {
    pub id: i64,
    pub assignment_id: Uuid,
    pub output_type: String,
    pub content: String,
    pub timestamp: DateTime<Utc>,
    #[allow(dead_code)]
    pub created_at: DateTime<Utc>,
}

impl From<RemoteLogRow> for LogEntry {
    fn from(row: RemoteLogRow) -> Self {
        Self {
            id: row.id,
            content: row.content,
            output_type: OutputType::from_remote_str(&row.output_type),
            timestamp: row.timestamp,
            execution_id: row.assignment_id,
        }
    }
}

/// Paginated response for log entries.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PaginatedLogs {
    /// The log entries for this page.
    pub entries: Vec<LogEntry>,

    /// Cursor for the next page (if more entries exist).
    pub next_cursor: Option<i64>,

    /// Whether there are more entries after this page.
    pub has_more: bool,

    /// Total count of log entries (if available).
    pub total_count: Option<i64>,
}

impl PaginatedLogs {
    /// Create a new PaginatedLogs response.
    pub fn new(
        entries: Vec<LogEntry>,
        next_cursor: Option<i64>,
        has_more: bool,
        total_count: Option<i64>,
    ) -> Self {
        Self {
            entries,
            next_cursor,
            has_more,
            total_count,
        }
    }

    /// Create an empty PaginatedLogs response.
    pub fn empty() -> Self {
        Self {
            entries: Vec::new(),
            next_cursor: None,
            has_more: false,
            total_count: Some(0),
        }
    }

    /// Add or update the total count in the pagination response.
    pub fn with_total_count(mut self, total: i64) -> Self {
        self.total_count = Some(total);
        self
    }
}

/// Direction for pagination queries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Direction {
    /// Fetch entries with IDs greater than cursor (oldest first).
    #[default]
    Forward,
    /// Fetch entries with IDs less than cursor (newest first).
    Backward,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_entry_from_local_jsonl_stdout() {
        let jsonl = r#"{"Stdout":"hello world"}"#;
        let execution_id = Uuid::new_v4();
        let timestamp = Utc::now();

        let entry = LogEntry::from_local_jsonl(jsonl, 1, execution_id, timestamp).unwrap();

        assert_eq!(entry.id, 1);
        assert_eq!(entry.content, "hello world");
        assert_eq!(entry.output_type, OutputType::Stdout);
        assert_eq!(entry.execution_id, execution_id);
    }

    #[test]
    fn test_log_entry_from_local_jsonl_stderr() {
        let jsonl = r#"{"Stderr":"error message"}"#;
        let execution_id = Uuid::new_v4();
        let timestamp = Utc::now();

        let entry = LogEntry::from_local_jsonl(jsonl, 2, execution_id, timestamp).unwrap();

        assert_eq!(entry.id, 2);
        assert_eq!(entry.content, "error message");
        assert_eq!(entry.output_type, OutputType::Stderr);
    }

    #[test]
    fn test_log_entry_from_local_jsonl_json_patch() {
        let jsonl = r#"{"JsonPatch":[{"op":"add","path":"/foo","value":"bar"}]}"#;
        let execution_id = Uuid::new_v4();
        let timestamp = Utc::now();

        let entry = LogEntry::from_local_jsonl(jsonl, 3, execution_id, timestamp).unwrap();

        assert_eq!(entry.id, 3);
        assert_eq!(entry.output_type, OutputType::JsonPatch);
        // Content should be the serialized patch
        assert!(entry.content.contains("add"));
        assert!(entry.content.contains("foo"));
    }

    #[test]
    fn test_log_entry_from_local_jsonl_session_id() {
        let jsonl = r#"{"SessionId":"abc123"}"#;
        let execution_id = Uuid::new_v4();
        let timestamp = Utc::now();

        let entry = LogEntry::from_local_jsonl(jsonl, 4, execution_id, timestamp).unwrap();

        assert_eq!(entry.id, 4);
        assert_eq!(entry.content, "abc123");
        assert_eq!(entry.output_type, OutputType::SessionId);
    }

    #[test]
    fn test_log_entry_from_local_jsonl_finished() {
        let jsonl = r#""Finished""#;
        let execution_id = Uuid::new_v4();
        let timestamp = Utc::now();

        let entry = LogEntry::from_local_jsonl(jsonl, 5, execution_id, timestamp).unwrap();

        assert_eq!(entry.id, 5);
        assert_eq!(entry.content, "");
        assert_eq!(entry.output_type, OutputType::Finished);
    }

    #[test]
    fn test_log_entry_from_local_jsonl_refresh_required() {
        let jsonl = r#"{"RefreshRequired":{"reason":"session expired"}}"#;
        let execution_id = Uuid::new_v4();
        let timestamp = Utc::now();

        let entry = LogEntry::from_local_jsonl(jsonl, 6, execution_id, timestamp).unwrap();

        assert_eq!(entry.id, 6);
        assert_eq!(entry.content, "session expired");
        assert_eq!(entry.output_type, OutputType::RefreshRequired);
    }

    #[test]
    fn test_log_entry_from_local_jsonl_invalid() {
        let jsonl = r#"{"Invalid":"data"}"#;
        let execution_id = Uuid::new_v4();
        let timestamp = Utc::now();

        let result = LogEntry::from_local_jsonl(jsonl, 1, execution_id, timestamp);
        assert!(result.is_err());
    }

    #[test]
    fn test_log_entry_from_remote_row() {
        let assignment_id = Uuid::new_v4();
        let timestamp = Utc::now();
        let created_at = Utc::now();

        let row = RemoteLogRow {
            id: 42,
            assignment_id,
            output_type: "stdout".to_string(),
            content: "hello from remote".to_string(),
            timestamp,
            created_at,
        };

        let entry = LogEntry::from(row);

        assert_eq!(entry.id, 42);
        assert_eq!(entry.content, "hello from remote");
        assert_eq!(entry.output_type, OutputType::Stdout);
        assert_eq!(entry.execution_id, assignment_id);
    }

    #[test]
    fn test_log_entry_from_remote_row_stderr() {
        let assignment_id = Uuid::new_v4();
        let timestamp = Utc::now();

        let row = RemoteLogRow {
            id: 100,
            assignment_id,
            output_type: "stderr".to_string(),
            content: "error from remote".to_string(),
            timestamp,
            created_at: Utc::now(),
        };

        let entry = LogEntry::from(row);

        assert_eq!(entry.id, 100);
        assert_eq!(entry.output_type, OutputType::Stderr);
    }

    #[test]
    fn test_log_entry_from_remote_row_unknown_type() {
        let assignment_id = Uuid::new_v4();
        let timestamp = Utc::now();

        let row = RemoteLogRow {
            id: 200,
            assignment_id,
            output_type: "unknown_type".to_string(),
            content: "some content".to_string(),
            timestamp,
            created_at: Utc::now(),
        };

        let entry = LogEntry::from(row);

        // Unknown types default to System
        assert_eq!(entry.output_type, OutputType::System);
    }

    #[test]
    fn test_log_entry_serialization() {
        let execution_id = Uuid::new_v4();
        let timestamp = Utc::now();

        let entry = LogEntry::new(
            1,
            "test content".to_string(),
            OutputType::Stdout,
            timestamp,
            execution_id,
        );

        let json = serde_json::to_string(&entry).unwrap();

        assert!(json.contains(r#""id":1"#));
        assert!(json.contains(r#""content":"test content""#));
        assert!(json.contains(r#""output_type":"stdout""#));
    }

    #[test]
    fn test_log_entry_deserialization() {
        let execution_id = Uuid::new_v4();
        let json = format!(
            r#"{{"id":5,"content":"hello","output_type":"stderr","timestamp":"2024-01-01T00:00:00Z","execution_id":"{}"}}"#,
            execution_id
        );

        let entry: LogEntry = serde_json::from_str(&json).unwrap();

        assert_eq!(entry.id, 5);
        assert_eq!(entry.content, "hello");
        assert_eq!(entry.output_type, OutputType::Stderr);
        assert_eq!(entry.execution_id, execution_id);
    }

    #[test]
    fn test_output_type_from_remote_str() {
        assert_eq!(OutputType::from_remote_str("stdout"), OutputType::Stdout);
        assert_eq!(OutputType::from_remote_str("stderr"), OutputType::Stderr);
        assert_eq!(OutputType::from_remote_str("system"), OutputType::System);
        assert_eq!(OutputType::from_remote_str("json_patch"), OutputType::JsonPatch);
        assert_eq!(OutputType::from_remote_str("session_id"), OutputType::SessionId);
        assert_eq!(OutputType::from_remote_str("finished"), OutputType::Finished);
        assert_eq!(OutputType::from_remote_str("refresh_required"), OutputType::RefreshRequired);
        assert_eq!(OutputType::from_remote_str("unknown"), OutputType::System);
    }

    #[test]
    fn test_output_type_as_str() {
        assert_eq!(OutputType::Stdout.as_str(), "stdout");
        assert_eq!(OutputType::Stderr.as_str(), "stderr");
        assert_eq!(OutputType::System.as_str(), "system");
        assert_eq!(OutputType::JsonPatch.as_str(), "json_patch");
        assert_eq!(OutputType::SessionId.as_str(), "session_id");
        assert_eq!(OutputType::Finished.as_str(), "finished");
        assert_eq!(OutputType::RefreshRequired.as_str(), "refresh_required");
    }

    #[test]
    fn test_to_log_msg_stdout() {
        let entry = LogEntry::new(
            1,
            "hello".to_string(),
            OutputType::Stdout,
            Utc::now(),
            Uuid::new_v4(),
        );

        let msg = entry.to_log_msg().unwrap();
        matches!(msg, LogMsg::Stdout(s) if s == "hello");
    }

    #[test]
    fn test_to_log_msg_stderr() {
        let entry = LogEntry::new(
            1,
            "error".to_string(),
            OutputType::Stderr,
            Utc::now(),
            Uuid::new_v4(),
        );

        let msg = entry.to_log_msg().unwrap();
        matches!(msg, LogMsg::Stderr(s) if s == "error");
    }

    #[test]
    fn test_paginated_logs_new() {
        let entries = vec![
            LogEntry::new(1, "a".to_string(), OutputType::Stdout, Utc::now(), Uuid::new_v4()),
            LogEntry::new(2, "b".to_string(), OutputType::Stdout, Utc::now(), Uuid::new_v4()),
        ];

        let paginated = PaginatedLogs::new(entries, Some(3), true, Some(100));

        assert_eq!(paginated.entries.len(), 2);
        assert_eq!(paginated.next_cursor, Some(3));
        assert!(paginated.has_more);
        assert_eq!(paginated.total_count, Some(100));
    }

    #[test]
    fn test_paginated_logs_empty() {
        let paginated = PaginatedLogs::empty();

        assert!(paginated.entries.is_empty());
        assert_eq!(paginated.next_cursor, None);
        assert!(!paginated.has_more);
        assert_eq!(paginated.total_count, Some(0));
    }

    #[test]
    fn test_direction_default() {
        let direction = Direction::default();
        assert_eq!(direction, Direction::Forward);
    }

    #[test]
    fn test_direction_serialization() {
        let forward = serde_json::to_string(&Direction::Forward).unwrap();
        let backward = serde_json::to_string(&Direction::Backward).unwrap();

        assert_eq!(forward, r#""forward""#);
        assert_eq!(backward, r#""backward""#);
    }
}
