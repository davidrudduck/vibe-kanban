//! Droid executor log normalization using the shared LogNormalizer trait.
//!
//! This module implements log normalization for the Droid executor,
//! converting DroidJson events into normalized conversation entries.

use std::{
    collections::{HashMap, VecDeque},
    path::{Path, PathBuf},
    sync::Arc,
};

use futures::StreamExt;
use json_patch::Patch;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use workspace_utils::{
    diff::{concatenate_diff_hunks, extract_unified_diff_hunks},
    msg_store::MsgStore,
    path::make_path_relative,
};

use crate::logs::{
    ActionType, CommandExitStatus, CommandRunResult, FileChange, NormalizedEntry,
    NormalizedEntryError, NormalizedEntryType, TodoItem, ToolResult, ToolStatus,
    normalizer::{LogNormalizer, normalize_logs_with},
    plain_text_processor::PlainTextLogProcessor,
    utils::{ConversationPatch, EntryIndexProvider},
};

/// Event types that can be parsed from Droid log lines.
#[derive(Debug, Clone)]
pub enum DroidEvent {
    /// A valid DroidJson event
    Json(DroidJson),
    /// An error log (level, message)
    ErrorLog(String),
    /// Non-JSON system output
    SystemOutput(String),
}

/// Normalizer for Droid executor logs.
///
/// Implements the `LogNormalizer` trait to process Droid protocol events
/// and convert them into normalized conversation entries.
pub struct DroidNormalizer {
    /// Path to the worktree for relative path resolution.
    worktree_path: PathBuf,
    /// Tool call state tracker.
    state: ToolCallStates,
    /// Whether we've already sent a completion message.
    sent_completion: bool,
}

impl DroidNormalizer {
    /// Create a new DroidNormalizer for the given worktree path.
    pub fn new(worktree_path: PathBuf) -> Self {
        Self {
            worktree_path,
            state: ToolCallStates::new(),
            sent_completion: false,
        }
    }
}

impl LogNormalizer for DroidNormalizer {
    type Event = DroidEvent;

    fn parse_line(&self, line: &str) -> Option<Self::Event> {
        let trimmed = line.trim();

        // Try to parse as DroidJson first
        if let Ok(droid_json) = serde_json::from_str::<DroidJson>(trimmed) {
            return Some(DroidEvent::Json(droid_json));
        }

        // Try to parse as error log
        if let Ok(DroidErrorLog { error, .. }) = serde_json::from_str::<DroidErrorLog>(trimmed) {
            return Some(DroidEvent::ErrorLog(error.message));
        }

        // Handle non-JSON output as raw system message
        if !trimmed.is_empty() {
            return Some(DroidEvent::SystemOutput(
                strip_ansi_escapes::strip_str(trimmed).to_string(),
            ));
        }

        None
    }

    fn extract_session_id(&self, event: &Self::Event) -> Option<String> {
        if let DroidEvent::Json(droid_json) = event {
            droid_json.session_id().map(|s| s.to_string())
        } else {
            None
        }
    }

    fn process_event(
        &mut self,
        event: Self::Event,
        msg_store: &Arc<MsgStore>,
        entry_index: &EntryIndexProvider,
    ) -> Vec<Patch> {
        match event {
            DroidEvent::ErrorLog(message) => {
                let idx = entry_index.next();
                let entry = NormalizedEntry {
                    timestamp: None,
                    entry_type: NormalizedEntryType::ErrorMessage {
                        error_type: NormalizedEntryError::Other,
                    },
                    content: message,
                    metadata: None,
                };
                vec![ConversationPatch::add_normalized_entry(idx, entry)]
            }
            DroidEvent::SystemOutput(content) => {
                let idx = entry_index.next();
                let entry = NormalizedEntry {
                    timestamp: None,
                    entry_type: NormalizedEntryType::SystemMessage,
                    content,
                    metadata: None,
                };
                vec![ConversationPatch::add_normalized_entry(idx, entry)]
            }
            DroidEvent::Json(droid_json) => {
                self.process_droid_json(droid_json, msg_store, entry_index)
            }
        }
    }
}

impl DroidNormalizer {
    /// Process a DroidJson event and return patches.
    fn process_droid_json(
        &mut self,
        droid_json: DroidJson,
        _msg_store: &Arc<MsgStore>,
        entry_index: &EntryIndexProvider,
    ) -> Vec<Patch> {
        // Convert to owned String to avoid borrow issues
        let worktree_path_str = self.worktree_path.to_string_lossy().into_owned();

        match droid_json {
            DroidJson::System { model, .. } => {
                if !self.state.model_reported
                    && let Some(model) = model
                {
                    self.state.model_reported = true;
                    let idx = entry_index.next();
                    let entry = NormalizedEntry {
                        timestamp: None,
                        entry_type: NormalizedEntryType::SystemMessage,
                        content: format!("model: {model}"),
                        metadata: None,
                    };
                    return vec![ConversationPatch::add_normalized_entry(idx, entry)];
                }
                vec![]
            }

            DroidJson::Message { role, text, .. } => {
                if role == "assistant" && self.sent_completion {
                    return vec![];
                }

                let entry_type = match role.as_str() {
                    "user" => NormalizedEntryType::UserMessage,
                    "assistant" => NormalizedEntryType::AssistantMessage,
                    _ => NormalizedEntryType::SystemMessage,
                };

                let idx = entry_index.next();
                let entry = NormalizedEntry {
                    timestamp: None,
                    entry_type,
                    content: text.clone(),
                    metadata: None,
                };
                vec![ConversationPatch::add_normalized_entry(idx, entry)]
            }

            DroidJson::ToolCall {
                id,
                tool_name,
                parameters: arguments,
                ..
            } => {
                self.process_tool_call(id, tool_name, arguments, entry_index, &worktree_path_str)
            }

            DroidJson::ToolResult {
                id: _,
                is_error,
                payload,
                ..
            } => self.process_tool_result(is_error, payload, &worktree_path_str),

            DroidJson::Completion { final_text, .. } => {
                let idx = entry_index.next();
                let entry = NormalizedEntry {
                    timestamp: None,
                    entry_type: NormalizedEntryType::AssistantMessage,
                    content: final_text.clone(),
                    metadata: None,
                };
                self.sent_completion = true;
                vec![ConversationPatch::add_normalized_entry(idx, entry)]
            }

            DroidJson::Error { message, .. } => {
                let idx = entry_index.next();
                let entry = NormalizedEntry {
                    timestamp: None,
                    entry_type: NormalizedEntryType::ErrorMessage {
                        error_type: NormalizedEntryError::Other,
                    },
                    content: message.clone(),
                    metadata: None,
                };
                vec![ConversationPatch::add_normalized_entry(idx, entry)]
            }
        }
    }

    /// Process a tool call and return patches.
    fn process_tool_call(
        &mut self,
        id: String,
        tool_name: String,
        arguments: Value,
        entry_index: &EntryIndexProvider,
        worktree_path_str: &str,
    ) -> Vec<Patch> {
        let tool_json = serde_json::json!({
            "toolName": tool_name,
            "parameters": arguments
        });

        let Ok(tool_data) = serde_json::from_value::<DroidToolData>(tool_json) else {
            tracing::warn!("Failed to parse tool parameters for {}", tool_name);
            return vec![];
        };

        match tool_data {
            DroidToolData::Read { file_path }
            | DroidToolData::LS {
                directory_path: file_path,
                ..
            } => {
                let path = make_path_relative(&file_path, worktree_path_str);
                let tool_state = FileReadState {
                    index: None,
                    path: path.clone(),
                    status: ToolStatus::Created,
                };
                self.state.file_reads.insert(id.to_string(), tool_state);
                self.state.pending_fifo.push_back(PendingToolCall::Read {
                    tool_call_id: id.to_string(),
                });
                let tool_state = self.state.file_reads.get_mut(&id).unwrap();
                let idx = entry_index.next();
                let entry = tool_state.to_normalized_entry();
                tool_state.index = Some(idx);
                vec![ConversationPatch::add_normalized_entry(idx, entry)]
            }

            DroidToolData::Grep {
                path: file_path, ..
            } => {
                let path = file_path
                    .as_ref()
                    .map(|p| make_path_relative(p, worktree_path_str))
                    .unwrap_or_default();
                let tool_state = FileReadState {
                    index: None,
                    path: path.clone(),
                    status: ToolStatus::Created,
                };
                self.state.file_reads.insert(id.clone(), tool_state);
                self.state.pending_fifo.push_back(PendingToolCall::Read {
                    tool_call_id: id.clone(),
                });
                let tool_state = self.state.file_reads.get_mut(&id).unwrap();
                let idx = entry_index.next();
                let entry = tool_state.to_normalized_entry();
                tool_state.index = Some(idx);
                vec![ConversationPatch::add_normalized_entry(idx, entry)]
            }

            DroidToolData::Glob { patterns, .. } => {
                let query = patterns.join(", ");
                let tool_state = SearchState {
                    index: None,
                    query: query.clone(),
                    status: ToolStatus::Created,
                };
                self.state.searches.insert(id.clone(), tool_state);
                self.state.pending_fifo.push_back(PendingToolCall::Search {
                    tool_call_id: id.clone(),
                });
                let tool_state = self.state.searches.get_mut(&id).unwrap();
                let idx = entry_index.next();
                let entry = tool_state.to_normalized_entry();
                tool_state.index = Some(idx);
                vec![ConversationPatch::add_normalized_entry(idx, entry)]
            }

            DroidToolData::Execute { command, .. } => {
                let tool_state = CommandRunState {
                    index: None,
                    command: command.clone(),
                    output: String::new(),
                    status: ToolStatus::Created,
                    exit_code: None,
                };
                self.state.command_runs.insert(id.clone(), tool_state);
                self.state
                    .pending_fifo
                    .push_back(PendingToolCall::CommandRun {
                        tool_call_id: id.clone(),
                    });
                let tool_state = self.state.command_runs.get_mut(&id).unwrap();
                let idx = entry_index.next();
                let entry = tool_state.to_normalized_entry();
                tool_state.index = Some(idx);
                vec![ConversationPatch::add_normalized_entry(idx, entry)]
            }

            DroidToolData::Edit {
                file_path,
                old_string,
                new_string,
            } => {
                let path = make_path_relative(&file_path, worktree_path_str);
                let diff =
                    workspace_utils::diff::create_unified_diff(&file_path, &old_string, &new_string);
                let changes = vec![FileChange::Edit {
                    unified_diff: diff,
                    has_line_numbers: false,
                }];

                let tool_state = FileEditState {
                    index: None,
                    path: path.clone(),
                    changes: changes.clone(),
                    status: ToolStatus::Created,
                };
                self.state.file_edits.insert(id.clone(), tool_state);
                self.state.pending_fifo.push_back(PendingToolCall::FileEdit {
                    tool_call_id: id.clone(),
                });
                let tool_state = self.state.file_edits.get_mut(&id).unwrap();
                let idx = entry_index.next();
                let entry = tool_state.to_normalized_entry();
                tool_state.index = Some(idx);
                vec![ConversationPatch::add_normalized_entry(idx, entry)]
            }

            DroidToolData::MultiEdit { file_path, edits } => {
                let path = make_path_relative(&file_path, worktree_path_str);
                let changes: Vec<FileChange> = edits
                    .iter()
                    .filter_map(|edit| {
                        if edit.old_string.is_some() || edit.new_string.is_some() {
                            Some(FileChange::Edit {
                                unified_diff: workspace_utils::diff::create_unified_diff(
                                    &file_path,
                                    &edit.old_string.clone().unwrap_or_default(),
                                    &edit.new_string.clone().unwrap_or_default(),
                                ),
                                has_line_numbers: false,
                            })
                        } else {
                            None
                        }
                    })
                    .collect();

                let tool_state = FileEditState {
                    index: None,
                    path: path.clone(),
                    changes: changes.clone(),
                    status: ToolStatus::Created,
                };
                self.state.file_edits.insert(id.clone(), tool_state);
                self.state.pending_fifo.push_back(PendingToolCall::FileEdit {
                    tool_call_id: id.clone(),
                });
                let tool_state = self.state.file_edits.get_mut(&id).unwrap();
                let idx = entry_index.next();
                let entry = tool_state.to_normalized_entry();
                tool_state.index = Some(idx);
                vec![ConversationPatch::add_normalized_entry(idx, entry)]
            }

            DroidToolData::Create { file_path, content } => {
                let path = make_path_relative(&file_path, worktree_path_str);
                let changes = vec![FileChange::Write { content }];

                let tool_state = FileEditState {
                    index: None,
                    path: path.clone(),
                    changes: changes.clone(),
                    status: ToolStatus::Created,
                };
                self.state.file_edits.insert(id.clone(), tool_state);
                self.state.pending_fifo.push_back(PendingToolCall::FileEdit {
                    tool_call_id: id.clone(),
                });

                let tool_state = self.state.file_edits.get_mut(&id).unwrap();
                let idx = entry_index.next();
                let entry = tool_state.to_normalized_entry();
                tool_state.index = Some(idx);
                vec![ConversationPatch::add_normalized_entry(idx, entry)]
            }

            DroidToolData::ApplyPatch { input } => {
                let path = extract_path_from_patch(&input);
                let path = make_path_relative(&path, worktree_path_str);

                // We get changes from tool result
                let tool_state = FileEditState {
                    index: None,
                    path: path.clone(),
                    changes: vec![],
                    status: ToolStatus::Created,
                };
                self.state.file_edits.insert(id.clone(), tool_state);
                self.state.pending_fifo.push_back(PendingToolCall::FileEdit {
                    tool_call_id: id.clone(),
                });

                let tool_state = self.state.file_edits.get_mut(&id).unwrap();
                let idx = entry_index.next();
                let entry = tool_state.to_normalized_entry();
                tool_state.index = Some(idx);
                vec![ConversationPatch::add_normalized_entry(idx, entry)]
            }

            DroidToolData::TodoWrite { todos } => {
                let todo_items: Vec<TodoItem> = todos
                    .into_iter()
                    .map(|item| TodoItem {
                        content: item.content,
                        status: item.status,
                        priority: item.priority,
                    })
                    .collect();

                let tool_state = TodoManagementState {
                    index: None,
                    todos: todo_items.clone(),
                    status: ToolStatus::Created,
                };
                self.state.todo_updates.insert(id.clone(), tool_state);
                self.state.pending_fifo.push_back(PendingToolCall::Todo {
                    tool_call_id: id.clone(),
                });

                let tool_state = self.state.todo_updates.get_mut(&id).unwrap();
                let idx = entry_index.next();
                let entry = tool_state.to_normalized_entry();
                tool_state.index = Some(idx);
                vec![ConversationPatch::add_normalized_entry(idx, entry)]
            }

            DroidToolData::WebSearch { query, .. } => {
                let tool_state = WebFetchState {
                    index: None,
                    url: query.clone(),
                    status: ToolStatus::Created,
                };
                self.state.web_fetches.insert(id.clone(), tool_state);
                self.state.pending_fifo.push_back(PendingToolCall::Fetch {
                    tool_call_id: id.clone(),
                });

                let tool_state = self.state.web_fetches.get_mut(&id).unwrap();
                let idx = entry_index.next();
                let entry = tool_state.to_normalized_entry();
                tool_state.index = Some(idx);
                vec![ConversationPatch::add_normalized_entry(idx, entry)]
            }

            DroidToolData::FetchUrl { url, .. } => {
                let tool_state = WebFetchState {
                    index: None,
                    url: url.clone(),
                    status: ToolStatus::Created,
                };
                self.state.web_fetches.insert(id.clone(), tool_state);
                self.state.pending_fifo.push_back(PendingToolCall::Fetch {
                    tool_call_id: id.clone(),
                });

                let tool_state = self.state.web_fetches.get_mut(&id).unwrap();
                let idx = entry_index.next();
                let entry = tool_state.to_normalized_entry();
                tool_state.index = Some(idx);
                vec![ConversationPatch::add_normalized_entry(idx, entry)]
            }

            DroidToolData::ExitSpecMode { .. } => {
                let tool_state = TodoManagementState {
                    index: None,
                    todos: vec![],
                    status: ToolStatus::Created,
                };
                self.state.todo_updates.insert(id.clone(), tool_state);
                self.state.pending_fifo.push_back(PendingToolCall::Todo {
                    tool_call_id: id.clone(),
                });
                let tool_state = self.state.todo_updates.get_mut(&id).unwrap();
                let idx = entry_index.next();
                let entry = tool_state.to_normalized_entry();
                tool_state.index = Some(idx);
                vec![ConversationPatch::add_normalized_entry(idx, entry)]
            }

            DroidToolData::SlackPostMessage { .. } | DroidToolData::Unknown { .. } => {
                let tool_state = GenericToolState {
                    index: None,
                    name: tool_name.to_string(),
                    arguments: Some(arguments.clone()),
                    result: None,
                    status: ToolStatus::Created,
                };
                self.state.generic_tools.insert(id.clone(), tool_state);
                self.state.pending_fifo.push_back(PendingToolCall::Generic {
                    tool_call_id: id.clone(),
                });

                let tool_state = self.state.generic_tools.get_mut(&id).unwrap();
                let idx = entry_index.next();
                let entry = tool_state.to_normalized_entry();
                tool_state.index = Some(idx);
                vec![ConversationPatch::add_normalized_entry(idx, entry)]
            }
        }
    }

    /// Process a tool result and return patches.
    fn process_tool_result(
        &mut self,
        is_error: bool,
        payload: ToolResultPayload,
        worktree_path_str: &str,
    ) -> Vec<Patch> {
        let Some(pending_tool_call) = self.state.pending_fifo.pop_front() else {
            return vec![];
        };

        match pending_tool_call {
            PendingToolCall::Read { tool_call_id } => {
                if let Some(mut state) = self.state.file_reads.remove(&tool_call_id) {
                    state.status = if is_error {
                        ToolStatus::Failed
                    } else {
                        ToolStatus::Success
                    };
                    let entry = state.to_normalized_entry();
                    return vec![ConversationPatch::replace(state.index.unwrap(), entry)];
                }
            }
            PendingToolCall::FileEdit { tool_call_id } => {
                if let Some(mut state) = self.state.file_edits.remove(&tool_call_id) {
                    state.status = if is_error {
                        ToolStatus::Failed
                    } else {
                        ToolStatus::Success
                    };

                    // Parse patch results if ApplyPatch tool
                    if let ToolResultPayload::Value { value } = &payload
                        && tool_call_id.contains("ApplyPatch")
                        && let Some(parsed) = parse_apply_patch_result(value, worktree_path_str)
                        && let ActionType::FileEdit { changes, .. } = parsed
                    {
                        state.changes = changes;
                    }

                    let entry = state.to_normalized_entry();
                    return vec![ConversationPatch::replace(state.index.unwrap(), entry)];
                }
            }
            PendingToolCall::CommandRun { tool_call_id } => {
                if let Some(mut state) = self.state.command_runs.remove(&tool_call_id) {
                    state.status = if is_error {
                        ToolStatus::Failed
                    } else {
                        ToolStatus::Success
                    };

                    match payload {
                        ToolResultPayload::Value { value } => {
                            let output = if let Some(s) = value.as_str() {
                                s.to_string()
                            } else {
                                serde_json::to_string_pretty(&value).unwrap_or_default()
                            };

                            let exit_code = output
                                .lines()
                                .find(|line| line.contains("[Process exited with code"))
                                .and_then(|line| {
                                    line.strip_prefix("[Process exited with code ")?
                                        .strip_suffix("]")?
                                        .parse::<i32>()
                                        .ok()
                                });

                            state.output = output;
                            state.exit_code = exit_code;
                            if exit_code.is_some_and(|rc| rc != 0) {
                                state.status = ToolStatus::Failed;
                            }
                        }
                        ToolResultPayload::Error { error } => {
                            state.output = error.message;
                        }
                    }

                    let entry = state.to_normalized_entry();
                    return vec![ConversationPatch::replace(state.index.unwrap(), entry)];
                }
            }
            PendingToolCall::Todo { tool_call_id } => {
                if let Some(mut state) = self.state.todo_updates.remove(&tool_call_id) {
                    state.status = if is_error {
                        ToolStatus::Failed
                    } else {
                        ToolStatus::Success
                    };
                    let entry = state.to_normalized_entry();
                    return vec![ConversationPatch::replace(state.index.unwrap(), entry)];
                }
            }
            PendingToolCall::Search { tool_call_id } => {
                if let Some(mut state) = self.state.searches.remove(&tool_call_id) {
                    state.status = if is_error {
                        ToolStatus::Failed
                    } else {
                        ToolStatus::Success
                    };
                    let entry = state.to_normalized_entry();
                    return vec![ConversationPatch::replace(state.index.unwrap(), entry)];
                }
            }
            PendingToolCall::Fetch { tool_call_id } => {
                if let Some(mut state) = self.state.web_fetches.remove(&tool_call_id) {
                    state.status = if is_error {
                        ToolStatus::Failed
                    } else {
                        ToolStatus::Success
                    };
                    let entry = state.to_normalized_entry();
                    return vec![ConversationPatch::replace(state.index.unwrap(), entry)];
                }
            }
            PendingToolCall::Generic { tool_call_id } => {
                if let Some(mut state) = self.state.generic_tools.remove(&tool_call_id) {
                    state.status = if is_error {
                        ToolStatus::Failed
                    } else {
                        ToolStatus::Success
                    };

                    match payload {
                        ToolResultPayload::Value { value } => {
                            state.result = Some(value);
                        }
                        ToolResultPayload::Error { error } => {
                            state.result = Some(error.message.into());
                        }
                    }

                    let entry = state.to_normalized_entry();
                    return vec![ConversationPatch::replace(state.index.unwrap(), entry)];
                }
            }
        }

        vec![]
    }
}

/// Main entry point for Droid log normalization.
///
/// Spawns background tasks to normalize both stdout (Droid events) and stderr logs.
/// The stdout normalization uses the shared `normalize_logs_with` driver function.
pub fn normalize_logs(
    msg_store: Arc<MsgStore>,
    worktree_path: &Path,
    entry_index_provider: EntryIndexProvider,
) -> tokio::task::JoinHandle<()> {
    let stderr_handle = normalize_stderr_logs(msg_store.clone(), entry_index_provider.clone());

    // Create the normalizer and use the shared driver function
    let normalizer = DroidNormalizer::new(worktree_path.to_path_buf());
    let stdout_handle = normalize_logs_with(normalizer, msg_store, worktree_path);

    // Return a handle that awaits both normalization tasks
    tokio::spawn(async move {
        let _ = stderr_handle.await;
        let _ = stdout_handle.await;
    })
}

fn normalize_stderr_logs(
    msg_store: Arc<MsgStore>,
    entry_index_provider: EntryIndexProvider,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut stderr = msg_store.stderr_chunked_stream();

        let mut processor = PlainTextLogProcessor::builder()
            .normalized_entry_producer(Box::new(|content: String| NormalizedEntry {
                timestamp: None,
                entry_type: NormalizedEntryType::ErrorMessage {
                    error_type: NormalizedEntryError::Other,
                },
                content,
                metadata: None,
            }))
            .transform_lines(Box::new(|lines| {
                lines.iter_mut().for_each(|line| {
                    *line = strip_ansi_escapes::strip_str(&line);
                    // noisy, but seemingly harmless message happens when session is forked
                    if line.starts_with("Error fetching session ") {
                        line.clear();
                    }
                });
            }))
            .time_gap(std::time::Duration::from_secs(2))
            .index_provider(entry_index_provider)
            .build();

        while let Some(Ok(chunk)) = stderr.next().await {
            for patch in processor.process(chunk) {
                msg_store.push_patch(patch);
            }
        }
    })
}

/// Extract path from ApplyPatch input format
fn extract_path_from_patch(input: &str) -> String {
    for line in input.lines() {
        if line.starts_with("*** Update File:") || line.starts_with("*** Add File:") {
            return line
                .split(':')
                .nth(1)
                .map(|s| s.trim().to_string())
                .unwrap_or_default();
        }
    }
    String::new()
}

/// Parse ApplyPatch result to extract file changes
fn parse_apply_patch_result(value: &Value, worktree_path: &str) -> Option<ActionType> {
    let parsed_value;
    let result_obj = if value.is_object() {
        value
    } else if let Some(s) = value.as_str() {
        match serde_json::from_str::<Value>(s) {
            Ok(v) => {
                parsed_value = v;
                &parsed_value
            }
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    input = %s,
                    "Failed to parse apply_patch result string as JSON"
                );
                return None;
            }
        }
    } else {
        tracing::warn!(
            value_type = ?value,
            "apply_patch result is neither object nor string"
        );
        return None;
    };

    let file_path = result_obj
        .get("file_path")
        .or_else(|| result_obj.get("value").and_then(|v| v.get("file_path")))
        .and_then(|v: &Value| v.as_str())
        .map(|s| s.to_string())?;

    let diff = result_obj
        .get("diff")
        .or_else(|| result_obj.get("value").and_then(|v| v.get("diff")))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let content = result_obj
        .get("content")
        .or_else(|| result_obj.get("value").and_then(|v| v.get("content")))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let relative_path = make_path_relative(&file_path, worktree_path);

    let changes = if let Some(diff_text) = diff {
        let hunks = extract_unified_diff_hunks(&diff_text);
        vec![FileChange::Edit {
            unified_diff: concatenate_diff_hunks(&relative_path, &hunks),
            has_line_numbers: true,
        }]
    } else if let Some(content_text) = content {
        vec![FileChange::Write {
            content: content_text,
        }]
    } else {
        vec![]
    };

    Some(ActionType::FileEdit {
        path: relative_path,
        changes,
    })
}

#[derive(Deserialize, Serialize, Debug, Clone, PartialEq)]
pub struct ToolError {
    #[serde(rename = "type")]
    pub kind: String,
    pub message: String,
}

#[derive(Deserialize, Serialize, Debug, Clone, PartialEq)]
#[serde(untagged)]
pub enum ToolResultPayload {
    Value { value: Value },
    Error { error: ToolError },
}

pub struct EditToolResult {}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DroidJson {
    System {
        #[serde(default)]
        subtype: Option<String>,
        session_id: String,
        #[serde(default)]
        cwd: Option<String>,
        #[serde(default)]
        tools: Option<Vec<String>>,
        #[serde(default)]
        model: Option<String>,
    },
    Message {
        role: String,
        id: String,
        text: String,
        timestamp: u64,
        session_id: String,
    },
    ToolCall {
        id: String,
        #[serde(rename = "messageId")]
        message_id: String,
        #[serde(rename = "toolId")]
        tool_id: String,
        #[serde(rename = "toolName")]
        tool_name: String,
        parameters: Value,
        timestamp: u64,
        session_id: String,
    },
    ToolResult {
        #[serde(default)]
        id: Option<String>,
        #[serde(rename = "messageId")]
        message_id: String,
        #[serde(rename = "toolId")]
        tool_id: String,
        #[serde(rename = "isError")]
        is_error: bool,
        #[serde(flatten)]
        payload: ToolResultPayload,
        timestamp: u64,
        session_id: String,
    },
    Error {
        source: String,
        message: String,
        timestamp: u64,
    },
    Completion {
        #[serde(rename = "finalText")]
        final_text: String,
        #[serde(default, rename = "numTurns")]
        num_turns: Option<u32>,
        #[serde(default, rename = "durationMs")]
        duration_ms: Option<u64>,
        #[serde(default)]
        timestamp: Option<u64>,
        session_id: String,
    },
}

#[derive(Deserialize, Serialize, Debug, Clone, PartialEq)]
struct DroidErrorLog {
    pub level: String,
    pub error: DroidErrorDetail,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub tags: Option<serde_json::Value>,
    #[serde(default)]
    pub msg: Option<String>,
}

#[derive(Deserialize, Serialize, Debug, Clone, PartialEq)]
struct DroidErrorDetail {
    #[serde(default)]
    pub name: Option<String>,
    pub message: String,
    #[serde(default)]
    pub stack: Option<String>,
}

impl DroidJson {
    pub fn session_id(&self) -> Option<&str> {
        match self {
            DroidJson::System { session_id, .. } => Some(session_id),
            DroidJson::Message { session_id, .. } => Some(session_id),
            DroidJson::ToolCall { session_id, .. } => Some(session_id),
            DroidJson::ToolResult { session_id, .. } => Some(session_id),
            DroidJson::Completion { session_id, .. } => Some(session_id),
            DroidJson::Error { .. } => None,
        }
    }
}

#[derive(Deserialize, Serialize, Debug, Clone, PartialEq)]
#[serde(tag = "toolName", content = "parameters")]
pub enum DroidToolData {
    Read {
        #[serde(alias = "path")]
        file_path: String,
    },
    LS {
        directory_path: String,
        #[serde(default)]
        #[serde(rename = "ignorePatterns")]
        ignore_patterns: Option<Vec<String>>,
    },
    Glob {
        folder: String,
        patterns: Vec<String>,
        #[serde(default)]
        #[serde(rename = "excludePatterns")]
        exclude_patterns: Option<Vec<String>>,
    },
    Grep {
        pattern: String,
        #[serde(default)]
        path: Option<String>,
        #[serde(default)]
        #[serde(rename = "caseSensitive")]
        case_sensitive: Option<bool>,
    },
    Execute {
        command: String,
        #[serde(default)]
        timeout: Option<u64>,
        #[serde(default)]
        #[serde(rename = "riskLevel")]
        risk_level: Option<Value>,
    },
    Edit {
        #[serde(alias = "path")]
        file_path: String,
        #[serde(alias = "old_str")]
        old_string: String,
        #[serde(alias = "new_str")]
        new_string: String,
    },
    MultiEdit {
        #[serde(alias = "path")]
        file_path: String,
        #[serde(alias = "changes")]
        edits: Vec<DroidEditItem>,
    },
    Create {
        #[serde(alias = "path")]
        file_path: String,
        content: String,
    },
    ApplyPatch {
        input: String,
    },
    TodoWrite {
        todos: Vec<DroidTodoItem>,
    },
    WebSearch {
        query: String,
        #[serde(default)]
        max_results: Option<u32>,
    },
    FetchUrl {
        url: String,
        #[serde(default)]
        method: Option<String>,
    },
    ExitSpecMode {
        #[serde(default)]
        reason: Option<String>,
    },
    #[serde(rename = "slack_post_message")]
    SlackPostMessage {
        channel: String,
        text: String,
    },
    #[serde(untagged)]
    Unknown {
        #[serde(flatten)]
        data: std::collections::HashMap<String, Value>,
    },
}

#[derive(Deserialize, Serialize, Debug, Clone, PartialEq)]
pub struct DroidTodoItem {
    #[serde(default)]
    pub id: Option<String>,
    pub content: String,
    pub status: String,
    #[serde(default)]
    pub priority: Option<String>,
}

#[derive(Deserialize, Serialize, Debug, Clone, PartialEq)]
pub struct DroidEditItem {
    pub old_string: Option<String>,
    pub new_string: Option<String>,
}

trait ToNormalizedEntry {
    fn to_normalized_entry(&self) -> NormalizedEntry;
}

#[derive(Debug, Clone)]
struct FileReadState {
    index: Option<usize>,
    path: String,
    status: ToolStatus,
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

#[derive(Debug, Clone)]
struct FileEditState {
    index: Option<usize>,
    path: String,
    changes: Vec<FileChange>,
    status: ToolStatus,
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

#[derive(Debug, Clone)]
struct CommandRunState {
    index: Option<usize>,
    command: String,
    output: String,
    status: ToolStatus,
    exit_code: Option<i32>,
}

impl ToNormalizedEntry for CommandRunState {
    fn to_normalized_entry(&self) -> NormalizedEntry {
        let result = if self.output.is_empty() && self.exit_code.is_none() {
            None
        } else {
            Some(CommandRunResult {
                exit_status: self
                    .exit_code
                    .map(|code| CommandExitStatus::ExitCode { code }),
                output: if self.output.is_empty() {
                    None
                } else {
                    Some(self.output.clone())
                },
            })
        };

        NormalizedEntry {
            timestamp: None,
            entry_type: NormalizedEntryType::ToolUse {
                tool_name: "bash".to_string(),
                action_type: ActionType::CommandRun {
                    command: self.command.clone(),
                    result,
                },
                status: self.status.clone(),
            },
            content: format!("`{}`", self.command),
            metadata: None,
        }
    }
}

#[derive(Debug, Clone)]
struct TodoManagementState {
    index: Option<usize>,
    todos: Vec<TodoItem>,
    status: ToolStatus,
}

impl ToNormalizedEntry for TodoManagementState {
    fn to_normalized_entry(&self) -> NormalizedEntry {
        let content = if self.todos.is_empty() {
            "TODO list updated".to_string()
        } else {
            format!("TODO list updated ({} items)", self.todos.len())
        };

        NormalizedEntry {
            timestamp: None,
            entry_type: NormalizedEntryType::ToolUse {
                tool_name: "todo".to_string(),
                action_type: ActionType::TodoManagement {
                    todos: self.todos.clone(),
                    operation: "update".to_string(),
                },
                status: self.status.clone(),
            },
            content,
            metadata: None,
        }
    }
}

#[derive(Debug, Clone)]
struct SearchState {
    index: Option<usize>,
    query: String,
    status: ToolStatus,
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

#[derive(Debug, Clone)]
struct WebFetchState {
    index: Option<usize>,
    url: String,
    status: ToolStatus,
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

#[derive(Debug, Clone)]
struct GenericToolState {
    index: Option<usize>,
    name: String,
    arguments: Option<Value>,
    status: ToolStatus,
    result: Option<Value>,
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
                            ToolResult::markdown(str)
                        } else {
                            ToolResult::json(value)
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

type ToolCallId = String;

#[derive(Debug, Clone)]
enum PendingToolCall {
    Read { tool_call_id: ToolCallId },
    FileEdit { tool_call_id: ToolCallId },
    CommandRun { tool_call_id: ToolCallId },
    Todo { tool_call_id: ToolCallId },
    Search { tool_call_id: ToolCallId },
    Fetch { tool_call_id: ToolCallId },
    Generic { tool_call_id: ToolCallId },
}

// Tracks tool-calls from creation to completion updating tool arguments and results as they come in
#[derive(Debug, Clone)]
struct ToolCallStates {
    file_reads: HashMap<String, FileReadState>,
    file_edits: HashMap<String, FileEditState>,
    command_runs: HashMap<String, CommandRunState>,
    todo_updates: HashMap<String, TodoManagementState>,
    searches: HashMap<String, SearchState>,
    web_fetches: HashMap<String, WebFetchState>,
    generic_tools: HashMap<String, GenericToolState>,
    pending_fifo: VecDeque<PendingToolCall>,
    model_reported: bool,
}

impl ToolCallStates {
    fn new() -> Self {
        Self {
            file_reads: HashMap::new(),
            file_edits: HashMap::new(),
            command_runs: HashMap::new(),
            todo_updates: HashMap::new(),
            searches: HashMap::new(),
            web_fetches: HashMap::new(),
            generic_tools: HashMap::new(),
            pending_fifo: VecDeque::new(),
            model_reported: false,
        }
    }
}
