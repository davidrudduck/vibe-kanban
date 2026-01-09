//! Shared tool state structures for log normalization.
//!
//! This module provides common state structures used across multiple executor
//! log normalizers (codex, droid, acp) to track tool call progress and convert
//! them to normalized log entries.

use crate::logs::{
    ActionType, CommandExitStatus, CommandRunResult, FileChange, NormalizedEntry,
    NormalizedEntryType, ToolResult, ToolResultValueType, ToolStatus,
};

/// Trait for converting tool states to normalized entries.
pub trait ToNormalizedEntry {
    fn to_normalized_entry(&self) -> NormalizedEntry;
}

/// State for tracking bash/command execution.
#[derive(Debug, Clone, Default)]
pub struct CommandState {
    /// Index in the normalized entry list (for updates).
    pub index: Option<usize>,
    /// The command being executed.
    pub command: String,
    /// Stdout output collected so far.
    pub stdout: String,
    /// Stderr output collected so far.
    pub stderr: String,
    /// Formatted output (preferred over stdout/stderr if available).
    pub formatted_output: Option<String>,
    /// Current tool status.
    pub status: ToolStatus,
    /// Exit code if the command has completed.
    pub exit_code: Option<i32>,
    /// Whether the command is awaiting user approval.
    pub awaiting_approval: bool,
    /// Unique call identifier for this tool call.
    pub call_id: String,
}

impl ToNormalizedEntry for CommandState {
    fn to_normalized_entry(&self) -> NormalizedEntry {
        let content = format!("`{}`", self.command);

        NormalizedEntry {
            timestamp: None,
            entry_type: NormalizedEntryType::ToolUse {
                tool_name: "bash".to_string(),
                action_type: ActionType::CommandRun {
                    command: self.command.clone(),
                    result: Some(CommandRunResult {
                        exit_status: self
                            .exit_code
                            .map(|code| CommandExitStatus::ExitCode { code }),
                        output: if self.formatted_output.is_some() {
                            self.formatted_output.clone()
                        } else {
                            build_command_output(Some(&self.stdout), Some(&self.stderr))
                        },
                    }),
                },
                status: self.status.clone(),
            },
            content,
            metadata: None,
        }
    }
}

/// State for tracking file read operations.
#[derive(Debug, Clone, Default)]
pub struct FileReadState {
    /// Index in the normalized entry list.
    pub index: Option<usize>,
    /// Path of the file being read (relative to worktree).
    pub path: String,
    /// Current tool status.
    pub status: ToolStatus,
}

impl ToNormalizedEntry for FileReadState {
    fn to_normalized_entry(&self) -> NormalizedEntry {
        NormalizedEntry {
            timestamp: None,
            entry_type: NormalizedEntryType::ToolUse {
                tool_name: "read".to_string(),
                action_type: ActionType::FileRead {
                    path: self.path.clone(),
                },
                status: self.status.clone(),
            },
            content: format!("`{}`", self.path),
            metadata: None,
        }
    }
}

/// State for tracking file edit operations.
#[derive(Debug, Clone, Default)]
pub struct FileEditState {
    /// Index in the normalized entry list.
    pub index: Option<usize>,
    /// Path of the file being edited (relative to worktree).
    pub path: String,
    /// The changes being applied to the file.
    pub changes: Vec<FileChange>,
    /// Current tool status.
    pub status: ToolStatus,
    /// Unique call identifier for this tool call.
    pub call_id: String,
}

impl ToNormalizedEntry for FileEditState {
    fn to_normalized_entry(&self) -> NormalizedEntry {
        NormalizedEntry {
            timestamp: None,
            entry_type: NormalizedEntryType::ToolUse {
                tool_name: "edit".to_string(),
                action_type: ActionType::FileEdit {
                    path: self.path.clone(),
                    changes: self.changes.clone(),
                },
                status: self.status.clone(),
            },
            content: format!("`{}`", self.path),
            metadata: None,
        }
    }
}

/// State for tracking web search/fetch operations.
#[derive(Debug, Clone, Default)]
pub struct WebFetchState {
    /// Index in the normalized entry list.
    pub index: Option<usize>,
    /// URL or query being fetched.
    pub url: String,
    /// Current tool status.
    pub status: ToolStatus,
}

impl ToNormalizedEntry for WebFetchState {
    fn to_normalized_entry(&self) -> NormalizedEntry {
        NormalizedEntry {
            timestamp: None,
            entry_type: NormalizedEntryType::ToolUse {
                tool_name: "fetch".to_string(),
                action_type: ActionType::WebFetch {
                    url: self.url.clone(),
                },
                status: self.status.clone(),
            },
            content: format!("`{}`", self.url),
            metadata: None,
        }
    }
}

/// State for tracking search operations (glob, grep).
#[derive(Debug, Clone, Default)]
pub struct SearchState {
    /// Index in the normalized entry list.
    pub index: Option<usize>,
    /// The search query or pattern.
    pub query: String,
    /// Current tool status.
    pub status: ToolStatus,
}

impl ToNormalizedEntry for SearchState {
    fn to_normalized_entry(&self) -> NormalizedEntry {
        NormalizedEntry {
            timestamp: None,
            entry_type: NormalizedEntryType::ToolUse {
                tool_name: "search".to_string(),
                action_type: ActionType::Search {
                    query: self.query.clone(),
                },
                status: self.status.clone(),
            },
            content: format!("`{}`", self.query),
            metadata: None,
        }
    }
}

/// State for tracking MCP tool calls.
#[derive(Debug, Clone, Default)]
pub struct McpToolState {
    /// Index in the normalized entry list.
    pub index: Option<usize>,
    /// Server name for the MCP tool.
    pub server: String,
    /// Tool name within the server.
    pub tool: String,
    /// Arguments passed to the tool.
    pub arguments: Option<serde_json::Value>,
    /// Result from the tool execution.
    pub result: Option<ToolResult>,
    /// Current tool status.
    pub status: ToolStatus,
}

impl ToNormalizedEntry for McpToolState {
    fn to_normalized_entry(&self) -> NormalizedEntry {
        let tool_name = format!("mcp:{}:{}", self.server, self.tool);
        NormalizedEntry {
            timestamp: None,
            entry_type: NormalizedEntryType::ToolUse {
                tool_name: tool_name.clone(),
                action_type: ActionType::Tool {
                    tool_name,
                    arguments: self.arguments.clone(),
                    result: self.result.clone(),
                },
                status: self.status.clone(),
            },
            content: self.tool.clone(),
            metadata: None,
        }
    }
}

/// State for tracking generic tool calls.
#[derive(Debug, Clone, Default)]
pub struct GenericToolState {
    /// Index in the normalized entry list.
    pub index: Option<usize>,
    /// Name of the tool.
    pub name: String,
    /// Arguments passed to the tool.
    pub arguments: Option<serde_json::Value>,
    /// Result from the tool execution.
    pub result: Option<serde_json::Value>,
    /// Current tool status.
    pub status: ToolStatus,
}

impl ToNormalizedEntry for GenericToolState {
    fn to_normalized_entry(&self) -> NormalizedEntry {
        NormalizedEntry {
            timestamp: None,
            entry_type: NormalizedEntryType::ToolUse {
                tool_name: self.name.clone(),
                action_type: ActionType::Tool {
                    tool_name: self.name.clone(),
                    arguments: self.arguments.clone(),
                    result: self.result.clone().map(|value| {
                        if let Some(str) = value.as_str() {
                            ToolResult {
                                r#type: ToolResultValueType::Markdown,
                                value: serde_json::Value::String(str.to_string()),
                            }
                        } else {
                            ToolResult {
                                r#type: ToolResultValueType::Json,
                                value,
                            }
                        }
                    }),
                },
                status: self.status.clone(),
            },
            content: self.name.clone(),
            metadata: None,
        }
    }
}

/// Mode for streaming text updates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateMode {
    /// Append new content to existing text.
    Append,
    /// Replace existing text with new content.
    Set,
}

/// Kind of streaming text (assistant message or thinking).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamingTextKind {
    /// Assistant message content.
    Assistant,
    /// Thinking/reasoning content.
    Thinking,
}

/// State for tracking streaming text output.
#[derive(Debug, Clone, Default)]
pub struct StreamingText {
    /// Index in the normalized entry list.
    pub index: usize,
    /// Accumulated text content.
    pub content: String,
}

impl StreamingText {
    /// Create a new streaming text state with the given index.
    pub fn new(index: usize) -> Self {
        Self {
            index,
            content: String::new(),
        }
    }

    /// Append content to the streaming text.
    pub fn append(&mut self, content: &str) {
        self.content.push_str(content);
    }

    /// Set the content (replacing any existing content).
    pub fn set(&mut self, content: String) {
        self.content = content;
    }

    /// Convert to a normalized entry of the given kind.
    pub fn to_normalized_entry(&self, kind: StreamingTextKind) -> NormalizedEntry {
        NormalizedEntry {
            timestamp: None,
            entry_type: match kind {
                StreamingTextKind::Assistant => NormalizedEntryType::AssistantMessage,
                StreamingTextKind::Thinking => NormalizedEntryType::Thinking,
            },
            content: self.content.clone(),
            metadata: None,
        }
    }
}

/// Build command output string from stdout and stderr.
pub fn build_command_output(stdout: Option<&str>, stderr: Option<&str>) -> Option<String> {
    let mut sections = Vec::new();
    if let Some(out) = stdout {
        let cleaned = out.trim();
        if !cleaned.is_empty() {
            sections.push(format!("stdout:\n{cleaned}"));
        }
    }
    if let Some(err) = stderr {
        let cleaned = err.trim();
        if !cleaned.is_empty() {
            sections.push(format!("stderr:\n{cleaned}"));
        }
    }

    if sections.is_empty() {
        None
    } else {
        Some(sections.join("\n\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_state_to_normalized_entry() {
        let state = CommandState {
            index: Some(0),
            command: "ls -la".to_string(),
            stdout: "file1\nfile2".to_string(),
            stderr: String::new(),
            formatted_output: None,
            status: ToolStatus::Success,
            exit_code: Some(0),
            awaiting_approval: false,
            call_id: "call-123".to_string(),
        };

        let entry = state.to_normalized_entry();

        // Verify entry type is ToolUse
        assert!(matches!(
            entry.entry_type,
            NormalizedEntryType::ToolUse { .. }
        ));

        // Verify tool name and action type
        if let NormalizedEntryType::ToolUse {
            tool_name,
            action_type,
            status,
        } = &entry.entry_type
        {
            assert_eq!(tool_name, "bash");
            assert!(matches!(action_type, ActionType::CommandRun { .. }));
            assert!(matches!(status, ToolStatus::Success));

            if let ActionType::CommandRun { command, result } = action_type {
                assert_eq!(command, "ls -la");
                assert!(result.is_some());
                let result = result.as_ref().unwrap();
                assert!(result.output.is_some());
                assert!(result.output.as_ref().unwrap().contains("file1"));
            }
        } else {
            panic!("Expected ToolUse entry type");
        }

        // Verify content format
        assert_eq!(entry.content, "`ls -la`");
    }

    #[test]
    fn test_command_state_with_formatted_output() {
        let state = CommandState {
            index: Some(0),
            command: "echo test".to_string(),
            stdout: "raw stdout".to_string(),
            stderr: "raw stderr".to_string(),
            formatted_output: Some("formatted output".to_string()),
            status: ToolStatus::Success,
            exit_code: Some(0),
            awaiting_approval: false,
            call_id: "call-456".to_string(),
        };

        let entry = state.to_normalized_entry();

        // Formatted output should take precedence
        if let NormalizedEntryType::ToolUse { action_type, .. } = &entry.entry_type {
            if let ActionType::CommandRun { result, .. } = action_type {
                let result = result.as_ref().unwrap();
                assert_eq!(result.output, Some("formatted output".to_string()));
            }
        }
    }

    #[test]
    fn test_file_edit_state_to_normalized_entry() {
        let state = FileEditState {
            index: Some(1),
            path: "src/main.rs".to_string(),
            changes: vec![FileChange::Edit {
                unified_diff: "--- a/src/main.rs\n+++ b/src/main.rs\n@@ -1,3 +1,4 @@\n+// New comment\n fn main() {\n".to_string(),
                has_line_numbers: true,
            }],
            status: ToolStatus::Success,
            call_id: "edit-123".to_string(),
        };

        let entry = state.to_normalized_entry();

        // Verify entry type is ToolUse
        assert!(matches!(
            entry.entry_type,
            NormalizedEntryType::ToolUse { .. }
        ));

        if let NormalizedEntryType::ToolUse {
            tool_name,
            action_type,
            status,
        } = &entry.entry_type
        {
            assert_eq!(tool_name, "edit");
            assert!(matches!(action_type, ActionType::FileEdit { .. }));
            assert!(matches!(status, ToolStatus::Success));

            if let ActionType::FileEdit { path, changes } = action_type {
                assert_eq!(path, "src/main.rs");
                assert_eq!(changes.len(), 1);
                assert!(matches!(changes[0], FileChange::Edit { .. }));
            }
        }

        assert_eq!(entry.content, "`src/main.rs`");
    }

    #[test]
    fn test_file_read_state_to_normalized_entry() {
        let state = FileReadState {
            index: Some(2),
            path: "README.md".to_string(),
            status: ToolStatus::Success,
        };

        let entry = state.to_normalized_entry();

        if let NormalizedEntryType::ToolUse {
            tool_name,
            action_type,
            status,
        } = &entry.entry_type
        {
            assert_eq!(tool_name, "read");
            assert!(matches!(action_type, ActionType::FileRead { .. }));
            assert!(matches!(status, ToolStatus::Success));

            if let ActionType::FileRead { path } = action_type {
                assert_eq!(path, "README.md");
            }
        }

        assert_eq!(entry.content, "`README.md`");
    }

    #[test]
    fn test_web_fetch_state_to_normalized_entry() {
        let state = WebFetchState {
            index: Some(3),
            url: "https://example.com".to_string(),
            status: ToolStatus::Success,
        };

        let entry = state.to_normalized_entry();

        if let NormalizedEntryType::ToolUse {
            tool_name,
            action_type,
            ..
        } = &entry.entry_type
        {
            assert_eq!(tool_name, "fetch");
            if let ActionType::WebFetch { url } = action_type {
                assert_eq!(url, "https://example.com");
            }
        }
    }

    #[test]
    fn test_search_state_to_normalized_entry() {
        let state = SearchState {
            index: Some(4),
            query: "*.rs".to_string(),
            status: ToolStatus::Success,
        };

        let entry = state.to_normalized_entry();

        if let NormalizedEntryType::ToolUse {
            tool_name,
            action_type,
            ..
        } = &entry.entry_type
        {
            assert_eq!(tool_name, "search");
            if let ActionType::Search { query } = action_type {
                assert_eq!(query, "*.rs");
            }
        }
    }

    #[test]
    fn test_mcp_tool_state_to_normalized_entry() {
        let state = McpToolState {
            index: Some(5),
            server: "github".to_string(),
            tool: "get_issues".to_string(),
            arguments: Some(serde_json::json!({"repo": "test/repo"})),
            result: Some(ToolResult {
                r#type: ToolResultValueType::Json,
                value: serde_json::json!([{"id": 1, "title": "Issue 1"}]),
            }),
            status: ToolStatus::Success,
        };

        let entry = state.to_normalized_entry();

        if let NormalizedEntryType::ToolUse {
            tool_name,
            action_type,
            ..
        } = &entry.entry_type
        {
            assert_eq!(tool_name, "mcp:github:get_issues");
            if let ActionType::Tool {
                tool_name,
                arguments,
                result,
            } = action_type
            {
                assert_eq!(tool_name, "mcp:github:get_issues");
                assert!(arguments.is_some());
                assert!(result.is_some());
            }
        }

        assert_eq!(entry.content, "get_issues");
    }

    #[test]
    fn test_generic_tool_state_to_normalized_entry() {
        let state = GenericToolState {
            index: Some(6),
            name: "custom_tool".to_string(),
            arguments: Some(serde_json::json!({"param": "value"})),
            result: Some(serde_json::json!({"output": "result"})),
            status: ToolStatus::Success,
        };

        let entry = state.to_normalized_entry();

        if let NormalizedEntryType::ToolUse {
            tool_name,
            action_type,
            ..
        } = &entry.entry_type
        {
            assert_eq!(tool_name, "custom_tool");
            if let ActionType::Tool { result, .. } = action_type {
                assert!(result.is_some());
                let result = result.as_ref().unwrap();
                // JSON result should be of type Json
                assert!(matches!(result.r#type, ToolResultValueType::Json));
            }
        }
    }

    #[test]
    fn test_generic_tool_state_with_string_result() {
        let state = GenericToolState {
            index: Some(7),
            name: "text_tool".to_string(),
            arguments: None,
            result: Some(serde_json::json!("plain text result")),
            status: ToolStatus::Success,
        };

        let entry = state.to_normalized_entry();

        if let NormalizedEntryType::ToolUse { action_type, .. } = &entry.entry_type {
            if let ActionType::Tool { result, .. } = action_type {
                assert!(result.is_some());
                let result = result.as_ref().unwrap();
                // String result should be of type Markdown
                assert!(matches!(result.r#type, ToolResultValueType::Markdown));
            }
        }
    }

    #[test]
    fn test_streaming_text_append() {
        let mut streaming = StreamingText::new(0);
        assert_eq!(streaming.content, "");

        streaming.append("Hello");
        assert_eq!(streaming.content, "Hello");

        streaming.append(", world!");
        assert_eq!(streaming.content, "Hello, world!");
    }

    #[test]
    fn test_streaming_text_set() {
        let mut streaming = StreamingText::new(0);
        streaming.append("Initial content");
        assert_eq!(streaming.content, "Initial content");

        streaming.set("Replaced content".to_string());
        assert_eq!(streaming.content, "Replaced content");
    }

    #[test]
    fn test_streaming_text_to_normalized_entry_assistant() {
        let mut streaming = StreamingText::new(0);
        streaming.set("Assistant message content".to_string());

        let entry = streaming.to_normalized_entry(StreamingTextKind::Assistant);

        assert!(matches!(
            entry.entry_type,
            NormalizedEntryType::AssistantMessage
        ));
        assert_eq!(entry.content, "Assistant message content");
    }

    #[test]
    fn test_streaming_text_to_normalized_entry_thinking() {
        let mut streaming = StreamingText::new(0);
        streaming.set("Thinking about the problem...".to_string());

        let entry = streaming.to_normalized_entry(StreamingTextKind::Thinking);

        assert!(matches!(entry.entry_type, NormalizedEntryType::Thinking));
        assert_eq!(entry.content, "Thinking about the problem...");
    }

    #[test]
    fn test_build_command_output_both_streams() {
        let output = build_command_output(Some("stdout content"), Some("stderr content"));
        assert!(output.is_some());
        let output = output.unwrap();
        assert!(output.contains("stdout:\nstdout content"));
        assert!(output.contains("stderr:\nstderr content"));
    }

    #[test]
    fn test_build_command_output_stdout_only() {
        let output = build_command_output(Some("stdout content"), None);
        assert!(output.is_some());
        assert_eq!(output.unwrap(), "stdout:\nstdout content");
    }

    #[test]
    fn test_build_command_output_empty() {
        let output = build_command_output(Some("  "), Some("  "));
        assert!(output.is_none());
    }

    #[test]
    fn test_command_state_with_exit_code() {
        let state = CommandState {
            exit_code: Some(1),
            status: ToolStatus::Failed,
            command: "false".to_string(),
            ..Default::default()
        };

        let entry = state.to_normalized_entry();

        if let NormalizedEntryType::ToolUse { action_type, .. } = &entry.entry_type {
            if let ActionType::CommandRun { result, .. } = action_type {
                let result = result.as_ref().unwrap();
                assert!(matches!(
                    result.exit_status,
                    Some(CommandExitStatus::ExitCode { code: 1 })
                ));
            }
        }
    }

    #[test]
    fn test_tool_state_default_status() {
        // Verify default status is Created for all tool states
        let cmd = CommandState::default();
        assert!(matches!(cmd.status, ToolStatus::Created));

        let file_read = FileReadState::default();
        assert!(matches!(file_read.status, ToolStatus::Created));

        let file_edit = FileEditState::default();
        assert!(matches!(file_edit.status, ToolStatus::Created));

        let web_fetch = WebFetchState::default();
        assert!(matches!(web_fetch.status, ToolStatus::Created));

        let search = SearchState::default();
        assert!(matches!(search.status, ToolStatus::Created));

        let mcp = McpToolState::default();
        assert!(matches!(mcp.status, ToolStatus::Created));

        let generic = GenericToolState::default();
        assert!(matches!(generic.status, ToolStatus::Created));
    }
}
