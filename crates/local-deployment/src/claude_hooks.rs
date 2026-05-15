use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{LocalDeployment, claude_terminal::tmux_session_name};
use std::collections::HashMap;

use db::models::{
    claude_terminal_session::ClaudeTerminalSession,
    coding_agent_turn::CodingAgentTurn,
    execution_process::{ExecutionProcess, ExecutionProcessStatus},
};
use executors::logs::{
    ActionType, AskUserQuestionItem, AskUserQuestionOption, CommandExitStatus, CommandRunResult,
    FileChange, NormalizedEntry, NormalizedEntryType, TodoItem, TokenUsageInfo, ToolResult,
    ToolResultValueType, ToolStatus,
    utils::{patch::ConversationPatch, shell_command_parsing::CommandCategory},
};
use services::services::container::ContainerService;
use tokio::fs;
use utils::{diff::create_unified_diff, path::make_path_relative};

#[derive(Debug, Clone, Default)]
pub struct ClaudeTerminalHookState {
    transcript_bytes_read: usize,
    pending_fragment: String,
    entry_count: usize,
    transcript_import: Option<TranscriptImportState>,
    session_id: Option<String>,
    completed: bool,
}

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

        self.upsert_claude_terminal_session_metadata(execution_id, &payload)
            .await?;

        let msg_store = {
            let stores = self.container.msg_stores().read().await;
            stores.get(&execution_id).cloned()
        };

        if let Some(msg_store) = msg_store {
            let mut state = {
                let mut states = self.claude_terminal_hooks.write().await;
                states.remove(&execution_id).unwrap_or_default()
            };

            if state.session_id.as_deref() != Some(payload.session_id.as_str()) {
                msg_store.push_session_id(payload.session_id.clone());
                state.session_id = Some(payload.session_id.clone());
            }

            if let Some(transcript_path) = payload.transcript_path.as_deref() {
                let transcript_offset = match ingest_transcript_delta(
                    &msg_store,
                    &mut state,
                    &payload,
                    transcript_path,
                )
                .await
                {
                    Ok(offset) => offset,
                    Err(err) => {
                        self.claude_terminal_hooks
                            .write()
                            .await
                            .insert(execution_id, state);
                        return Err(err);
                    }
                };
                if let Err(err) = ClaudeTerminalSession::update_transcript_offset(
                    &self.db.pool,
                    execution_id,
                    i64::try_from(transcript_offset).unwrap_or(i64::MAX),
                )
                .await
                {
                    self.claude_terminal_hooks
                        .write()
                        .await
                        .insert(execution_id, state);
                    return Err(err.into());
                }
            }

            let terminal_status = terminal_status_from_hook(&payload);
            let mut remove_state = false;

            if let Some(status) = terminal_status
                && !state.completed
            {
                let current_status =
                    ExecutionProcess::find_by_id(&self.db.pool, execution_id).await?;
                if !matches!(
                    current_status.as_ref().map(|process| &process.status),
                    Some(ExecutionProcessStatus::Running)
                ) {
                    state.completed = true;
                    remove_state = true;
                } else {
                    state.completed = true;
                    let exit_code = if status == ExecutionProcessStatus::Completed {
                        Some(0)
                    } else {
                        None
                    };
                    self.container.disable_terminal_monitor(execution_id).await;
                    if let Err(err) = self
                        .container
                        .terminate_terminal_execution_session(execution_id)
                        .await
                    {
                        self.claude_terminal_hooks
                            .write()
                            .await
                            .insert(execution_id, state);
                        return Err(err.into());
                    }
                    self.container
                        .complete_terminal_execution(execution_id, status, exit_code)
                        .await?;
                    remove_state = true;
                }
            }

            if !remove_state {
                self.claude_terminal_hooks
                    .write()
                    .await
                    .insert(execution_id, state);
            }
        }

        Ok(())
    }

    async fn upsert_claude_terminal_session_metadata(
        &self,
        execution_id: Uuid,
        payload: &ClaudeHookPayload,
    ) -> anyhow::Result<()> {
        let process = match ExecutionProcess::find_by_id(&self.db.pool, execution_id).await? {
            Some(process) => process,
            None => return Ok(()),
        };
        let Some((workspace, _)) = process.parent_workspace_and_session(&self.db.pool).await?
        else {
            return Ok(());
        };

        ClaudeTerminalSession::upsert_from_hook(
            &self.db.pool,
            execution_id,
            workspace.id,
            &tmux_session_name(execution_id),
            &payload.session_id,
            payload.transcript_path.as_deref(),
            payload.cwd.as_deref(),
        )
        .await?;
        CodingAgentTurn::update_agent_session_id(&self.db.pool, execution_id, &payload.session_id)
            .await?;

        Ok(())
    }
}

fn terminal_status_from_hook(payload: &ClaudeHookPayload) -> Option<ExecutionProcessStatus> {
    match payload.hook_event_name.as_str() {
        "Stop" => Some(ExecutionProcessStatus::Completed),
        "StopFailure" => Some(ExecutionProcessStatus::Failed),
        _ => None,
    }
}

async fn ingest_transcript_delta(
    msg_store: &std::sync::Arc<utils::msg_store::MsgStore>,
    state: &mut ClaudeTerminalHookState,
    payload: &ClaudeHookPayload,
    transcript_path: &str,
) -> anyhow::Result<usize> {
    let transcript = fs::read_to_string(transcript_path).await?;
    if transcript.len() < state.transcript_bytes_read {
        state.transcript_bytes_read = 0;
        state.pending_fragment.clear();
        state.entry_count = 0;
        state.transcript_import = None;
    }

    let previous_bytes_read = state.transcript_bytes_read;
    let delta = transcript.get(previous_bytes_read..).unwrap_or_default();
    let mut delta_with_pending = std::mem::take(&mut state.pending_fragment);
    let pending_len = delta_with_pending.len();
    let durable_offset_base = previous_bytes_read.saturating_sub(pending_len);
    delta_with_pending.push_str(delta);
    state.transcript_bytes_read = transcript.len();

    let mut rows = Vec::new();
    let mut consumed = 0;
    for segment in delta_with_pending.split_inclusive('\n') {
        let row = segment.trim_end_matches('\n');
        if row.is_empty() {
            consumed += segment.len();
            continue;
        }
        if serde_json::from_str::<serde_json::Value>(row).is_err() {
            break;
        }
        rows.push(row.to_string());
        consumed += segment.len();
    }

    if consumed < delta_with_pending.len() {
        state.pending_fragment = delta_with_pending[consumed..].to_string();
    }
    let durable_transcript_offset = durable_offset_base + consumed;

    let import_state = state.transcript_import.get_or_insert_with(|| {
        TranscriptImportState::new(200_000, payload.cwd.as_deref().unwrap_or_default())
    });
    if import_state.worktree_path.is_empty()
        && let Some(cwd) = payload.cwd.as_deref().filter(|cwd| !cwd.is_empty())
    {
        import_state.worktree_path = cwd.to_string();
    }

    let previous_entries = import_state.entries.clone();
    for row in rows {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&row) {
            import_state.ingest_value(&value);
        }
    }

    for (index, entry) in import_state.entries.iter().cloned().enumerate() {
        if let Some(previous) = previous_entries.get(index) {
            if normalized_entry_json(previous) != normalized_entry_json(&entry) {
                msg_store.push_patch(ConversationPatch::replace(index, entry));
            }
        } else {
            msg_store.push_patch(ConversationPatch::add_normalized_entry(index, entry));
        }
    }
    state.entry_count = import_state.entries.len();

    Ok(durable_transcript_offset)
}

fn normalized_entry_json(entry: &NormalizedEntry) -> serde_json::Value {
    serde_json::to_value(entry).unwrap_or(serde_json::Value::Null)
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

pub fn transcript_rows_to_entries(
    rows: &[String],
    model_context_window: u64,
) -> Vec<NormalizedEntry> {
    transcript_rows_to_entries_in_worktree(rows, model_context_window, "")
}

pub fn transcript_rows_to_entries_in_worktree(
    rows: &[String],
    model_context_window: u64,
    worktree_path: &str,
) -> Vec<NormalizedEntry> {
    let mut state = TranscriptImportState::new(model_context_window, worktree_path);
    for value in rows
        .iter()
        .filter_map(|row| serde_json::from_str::<serde_json::Value>(row).ok())
    {
        state.ingest_value(&value);
    }
    state.entries
}

#[derive(Debug, Clone)]
struct TranscriptImportState {
    entries: Vec<NormalizedEntry>,
    tool_entries: HashMap<String, usize>,
    model_context_window: u64,
    last_assistant_message: Option<String>,
    main_model_name: Option<String>,
    worktree_path: String,
}

impl TranscriptImportState {
    fn new(model_context_window: u64, worktree_path: &str) -> Self {
        Self {
            entries: Vec::new(),
            tool_entries: HashMap::new(),
            model_context_window,
            last_assistant_message: None,
            main_model_name: None,
            worktree_path: worktree_path.to_string(),
        }
    }

    fn ingest_value(&mut self, value: &serde_json::Value) {
        let Some(top_type) = value.get("type").and_then(|v| v.as_str()) else {
            return;
        };
        let timestamp = value
            .get("timestamp")
            .and_then(|v| v.as_str())
            .map(ToOwned::to_owned);

        match top_type {
            "assistant" => self.ingest_assistant(value, timestamp),
            "user" => self.ingest_user(value),
            "result" => self.ingest_result(value, timestamp),
            _ => {}
        }
    }

    fn ingest_assistant(&mut self, value: &serde_json::Value, timestamp: Option<String>) {
        let Some(message) = value.get("message") else {
            return;
        };
        if let Some(model) = message.get("model").and_then(|value| value.as_str()) {
            self.main_model_name = Some(model.to_string());
        }

        if let Some(content) = message.get("content") {
            for item in content.as_array().into_iter().flatten() {
                match item.get("type").and_then(|v| v.as_str()) {
                    Some("text") => {
                        if let Some(text) = item.get("text").and_then(|v| v.as_str())
                            && !text.is_empty()
                        {
                            self.entries.push(NormalizedEntry {
                                timestamp: timestamp.clone(),
                                entry_type: NormalizedEntryType::AssistantMessage,
                                content: text.to_string(),
                                metadata: Some(item.clone()),
                            });
                            self.last_assistant_message = Some(text.to_string());
                        }
                    }
                    Some("thinking") => {
                        if let Some(thinking) = item.get("thinking").and_then(|v| v.as_str())
                            && !thinking.is_empty()
                        {
                            self.entries.push(NormalizedEntry {
                                timestamp: timestamp.clone(),
                                entry_type: NormalizedEntryType::Thinking,
                                content: thinking.to_string(),
                                metadata: Some(item.clone()),
                            });
                        }
                    }
                    Some("tool_use") => self.ingest_tool_use(item, timestamp.clone()),
                    _ => {}
                }
            }
        }

        if let Some(usage) = message.get("usage") {
            self.push_token_usage(timestamp, usage, None);
        }
    }

    fn ingest_user(&mut self, value: &serde_json::Value) {
        let Some(content) = value
            .get("message")
            .and_then(|message| message.get("content"))
            .and_then(|content| content.as_array())
        else {
            return;
        };

        for item in content {
            if matches!(
                item.get("type").and_then(|value| value.as_str()),
                Some("tool_result")
            ) {
                self.ingest_tool_result(item);
            }
        }
    }

    fn ingest_result(&mut self, value: &serde_json::Value, timestamp: Option<String>) {
        let usage = value.get("usage");
        let model_context_window = value
            .get("modelUsage")
            .or_else(|| value.get("model_usage"))
            .and_then(|usage| extract_context_window(usage, self.main_model_name.as_deref()))
            .unwrap_or(self.model_context_window);
        let max_output_tokens = value
            .get("modelUsage")
            .or_else(|| value.get("model_usage"))
            .and_then(|usage| extract_max_output_tokens(usage, self.main_model_name.as_deref()));

        if let Some(usage) = usage {
            self.push_token_usage(
                timestamp.clone(),
                usage,
                Some(TerminalUsageFields {
                    model_context_window,
                    cost_microusd: value
                        .get("total_cost_usd")
                        .or_else(|| value.get("totalCostUsd"))
                        .and_then(|v| v.as_f64())
                        .and_then(cost_usd_to_microusd),
                    num_turns: value
                        .get("num_turns")
                        .or_else(|| value.get("numTurns"))
                        .and_then(|v| v.as_u64())
                        .and_then(|v| u32::try_from(v).ok()),
                    duration_ms: value
                        .get("duration_ms")
                        .or_else(|| value.get("durationMs"))
                        .and_then(|v| v.as_u64()),
                    max_output_tokens,
                }),
            );
        }

        if matches!(
            value.get("subtype").and_then(|v| v.as_str()),
            Some("success")
        ) && let Some(result) = value.get("result").and_then(|v| v.as_str())
            && !result.is_empty()
            && self
                .last_assistant_message
                .as_ref()
                .is_none_or(|message| !message.contains(result))
        {
            self.entries.push(NormalizedEntry {
                timestamp,
                entry_type: NormalizedEntryType::AssistantMessage,
                content: result.to_string(),
                metadata: None,
            });
            self.last_assistant_message = Some(result.to_string());
        }
    }

    fn ingest_tool_use(&mut self, item: &serde_json::Value, timestamp: Option<String>) {
        let Some(id) = item.get("id").and_then(|v| v.as_str()) else {
            return;
        };
        let raw_tool_name = item
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();
        let tool_name = canonical_tool_name(&raw_tool_name);
        let input = item
            .get("input")
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        let action_type = action_type_for_tool(&tool_name, &input, None, &self.worktree_path);
        let content = concise_tool_content(&tool_name, &input, &self.worktree_path);

        let entry = NormalizedEntry {
            timestamp,
            entry_type: NormalizedEntryType::ToolUse {
                tool_name: display_tool_name(&tool_name),
                action_type,
                status: ToolStatus::Created,
            },
            content,
            metadata: None,
        };

        if let Some(index) = self.tool_entries.get(id).copied() {
            self.entries[index] = entry;
        } else {
            self.tool_entries.insert(id.to_string(), self.entries.len());
            self.entries.push(entry);
        }
    }

    fn ingest_tool_result(&mut self, item: &serde_json::Value) {
        let Some(tool_use_id) = item.get("tool_use_id").and_then(|v| v.as_str()) else {
            return;
        };
        let Some(index) = self.tool_entries.get(tool_use_id).copied() else {
            return;
        };

        let result = item.get("content").unwrap_or(&serde_json::Value::Null);
        let is_error = item
            .get("is_error")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);

        let NormalizedEntryType::ToolUse {
            tool_name,
            action_type,
            status,
        } = &mut self.entries[index].entry_type
        else {
            return;
        };

        let command_result = if matches!(action_type, ActionType::CommandRun { .. }) {
            Some(command_run_result(result, is_error))
        } else {
            None
        };

        *status = if is_error || command_result.as_ref().is_some_and(command_result_failed) {
            ToolStatus::Failed
        } else {
            ToolStatus::Success
        };
        attach_tool_result(tool_name, action_type, result, is_error, command_result);
    }

    fn push_token_usage(
        &mut self,
        timestamp: Option<String>,
        usage: &serde_json::Value,
        terminal_fields: Option<TerminalUsageFields>,
    ) {
        let model_context_window = terminal_fields
            .as_ref()
            .map(|fields| fields.model_context_window)
            .unwrap_or(self.model_context_window);

        self.entries.push(NormalizedEntry {
            timestamp,
            entry_type: NormalizedEntryType::TokenUsageInfo(TokenUsageInfo {
                total_tokens: sum_usage_tokens(usage),
                model_context_window,
                output_tokens: usage.get("output_tokens").and_then(|v| v.as_u64()),
                cache_creation_tokens: usage
                    .get("cache_creation_input_tokens")
                    .and_then(|v| v.as_u64()),
                cache_read_tokens: usage
                    .get("cache_read_input_tokens")
                    .and_then(|v| v.as_u64()),
                cost_microusd: terminal_fields
                    .as_ref()
                    .and_then(|fields| fields.cost_microusd),
                num_turns: terminal_fields.as_ref().and_then(|fields| fields.num_turns),
                duration_ms: terminal_fields
                    .as_ref()
                    .and_then(|fields| fields.duration_ms),
                max_output_tokens: terminal_fields
                    .as_ref()
                    .and_then(|fields| fields.max_output_tokens),
            }),
            content: "Claude terminal token usage".to_string(),
            metadata: None,
        });
    }
}

#[derive(Debug, Clone, Copy)]
struct TerminalUsageFields {
    model_context_window: u64,
    cost_microusd: Option<u64>,
    num_turns: Option<u32>,
    duration_ms: Option<u64>,
    max_output_tokens: Option<u64>,
}

fn extract_text_content(content: &serde_json::Value) -> String {
    if let Some(text) = content.as_str() {
        return text.to_string();
    }

    content
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| item.get("text").and_then(|text| text.as_str()))
        .collect::<Vec<_>>()
        .join("\n")
}

fn action_type_for_tool(
    tool_name: &str,
    input: &serde_json::Value,
    result: Option<serde_json::Value>,
    worktree_path: &str,
) -> ActionType {
    match canonical_tool_name(tool_name).as_str() {
        "Bash" => {
            let command = input
                .get("command")
                .or_else(|| input.get("cmd"))
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            ActionType::CommandRun {
                category: CommandCategory::from_command(&command),
                command,
                result: result.and_then(|value| serde_json::from_value(value).ok()),
            }
        }
        "Read" => ActionType::FileRead {
            path: relative_input_path(input, worktree_path),
        },
        "Edit" => edit_action(input, worktree_path),
        "MultiEdit" => multi_edit_action(input, worktree_path),
        "Write" => write_action(input, worktree_path),
        "Grep" | "Glob" => ActionType::Search {
            query: input
                .get("pattern")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
        },
        "WebFetch" => ActionType::WebFetch {
            url: input
                .get("url")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
        },
        "WebSearch" => ActionType::WebFetch {
            url: input
                .get("query")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
        },
        "Task" => ActionType::TaskCreate {
            description: input
                .get("description")
                .or_else(|| input.get("prompt"))
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            subagent_type: input
                .get("subagent_type")
                .or_else(|| input.get("subagentType"))
                .and_then(|v| v.as_str())
                .map(ToOwned::to_owned),
            result: result.map(|value| normalize_tool_result(&value)),
        },
        "ExitPlanMode" => ActionType::PlanPresentation {
            plan: input
                .get("plan")
                .and_then(|value| value.as_str())
                .unwrap_or_default()
                .to_string(),
        },
        "TodoWrite" => ActionType::TodoManagement {
            todos: input
                .get("todos")
                .and_then(|value| value.as_array())
                .into_iter()
                .flatten()
                .map(|todo| TodoItem {
                    content: todo
                        .get("content")
                        .and_then(|value| value.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    status: todo
                        .get("status")
                        .and_then(|value| value.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    priority: todo
                        .get("priority")
                        .and_then(|value| value.as_str())
                        .map(ToOwned::to_owned),
                })
                .collect(),
            operation: "write".to_string(),
        },
        "TodoRead" => ActionType::TodoManagement {
            todos: vec![],
            operation: "read".to_string(),
        },
        "LS" => ActionType::Other {
            description: "List directory".to_string(),
        },
        "AskUserQuestion" => ActionType::AskUserQuestion {
            questions: input
                .get("questions")
                .and_then(|value| value.as_array())
                .into_iter()
                .flatten()
                .map(|question| AskUserQuestionItem {
                    question: question
                        .get("question")
                        .and_then(|value| value.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    header: question
                        .get("header")
                        .and_then(|value| value.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    options: question
                        .get("options")
                        .and_then(|value| value.as_array())
                        .into_iter()
                        .flatten()
                        .map(|option| AskUserQuestionOption {
                            label: option
                                .get("label")
                                .and_then(|value| value.as_str())
                                .unwrap_or_default()
                                .to_string(),
                            description: option
                                .get("description")
                                .and_then(|value| value.as_str())
                                .unwrap_or_default()
                                .to_string(),
                        })
                        .collect(),
                    multi_select: question
                        .get("multiSelect")
                        .or_else(|| question.get("multi_select"))
                        .and_then(|value| value.as_bool())
                        .unwrap_or(false),
                })
                .collect(),
        },
        _ => ActionType::Tool {
            tool_name: display_tool_name(tool_name),
            arguments: Some(input.clone()),
            result: result.map(|value| normalize_tool_result(&value)),
        },
    }
}

fn edit_action(input: &serde_json::Value, worktree_path: &str) -> ActionType {
    let file_path = input_path(input);
    let old_string = input
        .get("old_string")
        .or_else(|| input.get("old_str"))
        .and_then(|value| value.as_str())
        .unwrap_or_default();
    let new_string = input
        .get("new_string")
        .or_else(|| input.get("new_str"))
        .and_then(|value| value.as_str())
        .unwrap_or_default();
    let changes = if !old_string.is_empty() || !new_string.is_empty() {
        vec![FileChange::Edit {
            unified_diff: create_unified_diff(file_path, old_string, new_string),
            has_line_numbers: false,
        }]
    } else {
        vec![]
    };

    ActionType::FileEdit {
        path: relative_path(file_path, worktree_path),
        changes,
    }
}

fn multi_edit_action(input: &serde_json::Value, worktree_path: &str) -> ActionType {
    let file_path = input_path(input);
    let changes = input
        .get("edits")
        .and_then(|value| value.as_array())
        .into_iter()
        .flatten()
        .filter_map(|edit| {
            let old_string = edit
                .get("old_string")
                .or_else(|| edit.get("old_str"))
                .and_then(|value| value.as_str())
                .unwrap_or_default();
            let new_string = edit
                .get("new_string")
                .or_else(|| edit.get("new_str"))
                .and_then(|value| value.as_str())
                .unwrap_or_default();

            if old_string.is_empty() && new_string.is_empty() {
                None
            } else {
                Some(FileChange::Edit {
                    unified_diff: create_unified_diff(file_path, old_string, new_string),
                    has_line_numbers: false,
                })
            }
        })
        .collect();

    ActionType::FileEdit {
        path: relative_path(file_path, worktree_path),
        changes,
    }
}

fn write_action(input: &serde_json::Value, worktree_path: &str) -> ActionType {
    let file_path = input_path(input);
    ActionType::FileEdit {
        path: relative_path(file_path, worktree_path),
        changes: vec![FileChange::Write {
            content: input
                .get("content")
                .and_then(|value| value.as_str())
                .unwrap_or_default()
                .to_string(),
        }],
    }
}

fn relative_input_path(input: &serde_json::Value, worktree_path: &str) -> String {
    relative_path(input_path(input), worktree_path)
}

fn input_path(input: &serde_json::Value) -> &str {
    input
        .get("file_path")
        .or_else(|| input.get("path"))
        .and_then(|value| value.as_str())
        .unwrap_or_default()
}

fn relative_path(path: &str, worktree_path: &str) -> String {
    if worktree_path.is_empty() {
        return path.to_string();
    }
    make_path_relative(path, worktree_path)
}

fn canonical_tool_name(tool_name: &str) -> String {
    match tool_name {
        "bash" => "Bash",
        "read" => "Read",
        "edit_file" => "Edit",
        "multi_edit" => "MultiEdit",
        "create_file" | "write_file" => "Write",
        "grep" => "Grep",
        "glob" => "Glob",
        "list_directory" | "ls" => "LS",
        "read_web_page" => "WebFetch",
        "web_search" => "WebSearch",
        "todo_write" => "TodoWrite",
        "todo_read" => "TodoRead",
        "task" | "Agent" => "Task",
        other => other,
    }
    .to_string()
}

fn attach_tool_result(
    _tool_name: &str,
    action_type: &mut ActionType,
    result: &serde_json::Value,
    is_error: bool,
    parsed_command_result: Option<CommandRunResult>,
) {
    match action_type {
        ActionType::CommandRun {
            command,
            result: command_result,
            category,
        } => {
            *command_result =
                parsed_command_result.or_else(|| Some(command_run_result(result, is_error)));
            *category = CommandCategory::from_command(command);
        }
        ActionType::TaskCreate { result: res, .. } => {
            *res = Some(normalize_tool_result(result));
        }
        ActionType::Tool { result: res, .. } => {
            *res = Some(normalize_tool_result(result));
        }
        _ => {}
    }
}

fn command_result_failed(result: &CommandRunResult) -> bool {
    match result.exit_status.as_ref() {
        Some(CommandExitStatus::ExitCode { code }) => *code != 0,
        Some(CommandExitStatus::Success { success }) => !success,
        None => false,
    }
}

fn command_run_result(content: &serde_json::Value, is_error: bool) -> CommandRunResult {
    let content_str = extract_tool_result_content(content);
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(&content_str) {
        let output = value
            .get("output")
            .and_then(|value| value.as_str())
            .map(ToOwned::to_owned);
        let exit_code = value
            .get("exitCode")
            .or_else(|| value.get("exit_code"))
            .and_then(|value| value.as_i64())
            .and_then(|value| i32::try_from(value).ok());

        if output.is_some() || exit_code.is_some() {
            return CommandRunResult {
                exit_status: exit_code
                    .map(|code| CommandExitStatus::ExitCode { code })
                    .or(Some(CommandExitStatus::Success { success: !is_error })),
                output,
            };
        }
    }

    CommandRunResult {
        exit_status: Some(CommandExitStatus::Success { success: !is_error }),
        output: Some(content_str),
    }
}

fn extract_tool_result_content(content: &serde_json::Value) -> String {
    if let Some(text) = content.as_str() {
        return text.to_string();
    }

    let text = extract_text_content(content);
    if !text.is_empty() {
        return text;
    }

    content.to_string()
}

fn normalize_tool_result(content: &serde_json::Value) -> ToolResult {
    if let Some(text) = content.as_str() {
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(text) {
            return ToolResult::json(parsed);
        }
        return ToolResult::markdown(text.to_string());
    }

    if let Some(items) = content.as_array() {
        let joined = items
            .iter()
            .filter_map(|item| item.get("text").and_then(|value| value.as_str()))
            .collect::<Vec<_>>()
            .join("\n\n");
        if !joined.is_empty() {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&joined) {
                return ToolResult::json(parsed);
            }
            return ToolResult {
                r#type: ToolResultValueType::Markdown,
                value: serde_json::Value::String(joined),
            };
        }
    }

    ToolResult::json(content.clone())
}

fn concise_tool_content(tool_name: &str, input: &serde_json::Value, worktree_path: &str) -> String {
    match tool_name {
        "Bash" => input
            .get("command")
            .or_else(|| input.get("cmd"))
            .and_then(|value| value.as_str())
            .unwrap_or("Run command")
            .to_string(),
        "Read" => input
            .get("file_path")
            .or_else(|| input.get("path"))
            .and_then(|value| value.as_str())
            .map(|path| format!("Read {}", relative_path(path, worktree_path)))
            .unwrap_or_else(|| "Read file".to_string()),
        _ => display_tool_name(tool_name),
    }
}

fn display_tool_name(tool_name: &str) -> String {
    if tool_name.starts_with("mcp__") {
        let parts: Vec<&str> = tool_name.split("__").collect();
        if parts.len() >= 3 {
            return format!("mcp:{}:{}", parts[1], parts[2]);
        }
    }
    tool_name.to_string()
}

fn extract_context_window(
    model_usage: &serde_json::Value,
    model_name: Option<&str>,
) -> Option<u64> {
    extract_model_usage_u64(model_usage, model_name, "context_window", "contextWindow")
}

fn extract_max_output_tokens(
    model_usage: &serde_json::Value,
    model_name: Option<&str>,
) -> Option<u64> {
    extract_model_usage_u64(
        model_usage,
        model_name,
        "max_output_tokens",
        "maxOutputTokens",
    )
}

fn extract_model_usage_u64(
    model_usage: &serde_json::Value,
    model_name: Option<&str>,
    snake_key: &str,
    camel_key: &str,
) -> Option<u64> {
    let models = model_usage.as_object()?;

    if let Some(model_name) = model_name
        && let Some(value) = models
            .get(model_name)
            .and_then(|usage| usage_value(usage, snake_key, camel_key))
    {
        return Some(value);
    }

    models
        .iter()
        .filter_map(|(_, usage)| usage_value(usage, snake_key, camel_key))
        .max()
}

fn usage_value(usage: &serde_json::Value, snake_key: &str, camel_key: &str) -> Option<u64> {
    usage
        .get(snake_key)
        .or_else(|| usage.get(camel_key))
        .and_then(|value| value.as_u64())
}

fn cost_usd_to_microusd(cost: f64) -> Option<u64> {
    if cost >= 0.0 {
        Some((cost * 1_000_000.0).round() as u64)
    } else {
        None
    }
}

fn sum_usage_tokens(usage: &serde_json::Value) -> u64 {
    [
        "input_tokens",
        "cache_creation_input_tokens",
        "cache_read_input_tokens",
        "output_tokens",
    ]
    .iter()
    .filter_map(|key| usage.get(*key).and_then(|value| value.as_u64()))
    .sum()
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

    #[test]
    fn session_end_hook_does_not_complete_terminal_execution() {
        let payload: ClaudeHookPayload = serde_json::from_value(serde_json::json!({
            "session_id": "claude-session-123",
            "hook_event_name": "SessionEnd",
            "reason": "prompt_input_exit"
        }))
        .unwrap();

        assert!(terminal_status_from_hook(&payload).is_none());
    }

    #[test]
    fn stop_hooks_map_to_terminal_completion_statuses() {
        let stop: ClaudeHookPayload = serde_json::from_value(serde_json::json!({
            "session_id": "claude-session-123",
            "hook_event_name": "Stop"
        }))
        .unwrap();
        let stop_failure: ClaudeHookPayload = serde_json::from_value(serde_json::json!({
            "session_id": "claude-session-123",
            "hook_event_name": "StopFailure"
        }))
        .unwrap();

        assert_eq!(
            terminal_status_from_hook(&stop),
            Some(ExecutionProcessStatus::Completed)
        );
        assert_eq!(
            terminal_status_from_hook(&stop_failure),
            Some(ExecutionProcessStatus::Failed)
        );
    }

    #[test]
    fn imports_assistant_message_and_token_usage_from_transcript_rows() {
        let rows = vec![
            r#"{"type":"assistant","timestamp":"2026-05-15T00:00:00Z","message":{"role":"assistant","content":[{"type":"text","text":"Done."}],"usage":{"input_tokens":10,"cache_creation_input_tokens":2,"cache_read_input_tokens":3,"output_tokens":4}}}"#.to_string()
        ];

        let entries = transcript_rows_to_entries(&rows, 200000);

        assert!(entries.iter().any(|entry| matches!(
            entry.entry_type,
            executors::logs::NormalizedEntryType::AssistantMessage
        ) && entry.content == "Done."));

        assert!(entries.iter().any(|entry| matches!(
            entry.entry_type,
            executors::logs::NormalizedEntryType::TokenUsageInfo(_)
        )));

        assert!(entries.iter().all(|entry| {
            entry
                .metadata
                .as_ref()
                .is_none_or(|metadata| metadata.get("message").is_none())
        }));
    }

    #[test]
    fn skips_invalid_json_rows_and_prompt_echoes_without_failing_import() {
        let rows = vec![
            "not-json".to_string(),
            r#"{"type":"user","message":{"role":"user","content":"hello"}}"#.to_string(),
        ];

        let entries = transcript_rows_to_entries(&rows, 200000);

        assert!(entries.is_empty());
    }

    #[test]
    fn imports_tool_use_and_updates_it_from_tool_result() {
        let rows = vec![
            r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","id":"toolu_1","name":"Bash","input":{"command":"cargo test -p local-deployment"}}]}}"#.to_string(),
            r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_1","content":"ok","is_error":false}]}}"#.to_string(),
        ];

        let entries = transcript_rows_to_entries(&rows, 200000);

        assert_eq!(entries.len(), 1);
        let NormalizedEntryType::ToolUse {
            tool_name,
            action_type,
            status,
        } = &entries[0].entry_type
        else {
            panic!("expected tool use entry");
        };

        assert_eq!(tool_name, "Bash");
        assert!(matches!(status, ToolStatus::Success));
        let ActionType::CommandRun {
            command, result, ..
        } = action_type
        else {
            panic!("expected command run action");
        };
        assert_eq!(command, "cargo test -p local-deployment");
        assert_eq!(
            result.as_ref().and_then(|res| res.output.as_deref()),
            Some("ok")
        );
    }

    #[test]
    fn imports_terminal_result_usage_fields() {
        let rows = vec![
            r#"{"type":"result","subtype":"success","total_cost_usd":0.000123,"num_turns":3,"duration_ms":4567,"usage":{"input_tokens":10,"cache_creation_input_tokens":2,"cache_read_input_tokens":3,"output_tokens":4},"modelUsage":{"claude-sonnet-4-5":{"context_window":200000,"max_output_tokens":32000}}}"#.to_string()
        ];

        let entries = transcript_rows_to_entries(&rows, 100000);

        let Some(NormalizedEntry {
            entry_type: NormalizedEntryType::TokenUsageInfo(usage),
            ..
        }) = entries.first()
        else {
            panic!("expected token usage entry");
        };

        assert_eq!(usage.total_tokens, 19);
        assert_eq!(usage.model_context_window, 200000);
        assert_eq!(usage.cost_microusd, Some(123));
        assert_eq!(usage.num_turns, Some(3));
        assert_eq!(usage.duration_ms, Some(4567));
        assert_eq!(usage.max_output_tokens, Some(32000));
    }

    #[test]
    fn tool_result_keeps_original_non_bash_action_data() {
        let rows = vec![
            r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","id":"toolu_1","name":"Read","input":{"file_path":"/tmp/app.rs"}}]}}"#.to_string(),
            r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_1","content":"file contents","is_error":false}]}}"#.to_string(),
        ];

        let entries = transcript_rows_to_entries_in_worktree(&rows, 200000, "/tmp");

        let NormalizedEntryType::ToolUse {
            tool_name,
            action_type,
            status,
        } = &entries[0].entry_type
        else {
            panic!("expected tool use entry");
        };
        assert!(matches!(status, ToolStatus::Success));
        assert_eq!(tool_name, "Read");
        assert_eq!(entries[0].content, "Read app.rs");
        assert!(matches!(
            action_type,
            ActionType::FileRead { path } if path == "app.rs"
        ));
    }

    #[test]
    fn result_row_does_not_duplicate_last_assistant_message() {
        let rows = vec![
            r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Done."}]}}"#.to_string(),
            r#"{"type":"result","subtype":"success","result":"Done.","usage":{"input_tokens":1,"output_tokens":1}}"#.to_string(),
        ];

        let entries = transcript_rows_to_entries(&rows, 200000);
        let assistant_count = entries
            .iter()
            .filter(|entry| matches!(entry.entry_type, NormalizedEntryType::AssistantMessage))
            .count();

        assert_eq!(assistant_count, 1);
    }

    #[test]
    fn bash_tool_result_parses_exit_code_payload() {
        let rows = vec![
            r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","id":"toolu_1","name":"Bash","input":{"command":"false"}}]}}"#.to_string(),
            r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_1","content":"{\"output\":\"failed\\n\",\"exitCode\":1}"}]}}"#.to_string(),
        ];

        let entries = transcript_rows_to_entries(&rows, 200000);
        let NormalizedEntryType::ToolUse {
            action_type,
            status,
            ..
        } = &entries[0].entry_type
        else {
            panic!("expected tool use entry");
        };
        assert!(matches!(status, ToolStatus::Failed));
        let ActionType::CommandRun { result, .. } = action_type else {
            panic!("expected command action");
        };
        assert!(matches!(
            result.as_ref().and_then(|res| res.exit_status.as_ref()),
            Some(CommandExitStatus::ExitCode { code: 1 })
        ));
        assert_eq!(
            result.as_ref().and_then(|res| res.output.as_deref()),
            Some("failed\n")
        );
    }

    #[test]
    fn imports_camel_case_terminal_result_usage_fields() {
        let rows = vec![
            r#"{"type":"result","subtype":"success","totalCostUsd":0.000456,"numTurns":4,"durationMs":7890,"usage":{"input_tokens":10,"output_tokens":5},"modelUsage":{"claude-sonnet-4-5":{"contextWindow":200000,"maxOutputTokens":32000}}}"#.to_string()
        ];

        let entries = transcript_rows_to_entries(&rows, 100000);
        let Some(NormalizedEntry {
            entry_type: NormalizedEntryType::TokenUsageInfo(usage),
            ..
        }) = entries.first()
        else {
            panic!("expected token usage entry");
        };

        assert_eq!(usage.cost_microusd, Some(456));
        assert_eq!(usage.num_turns, Some(4));
        assert_eq!(usage.duration_ms, Some(7890));
        assert_eq!(usage.model_context_window, 200000);
        assert_eq!(usage.max_output_tokens, Some(32000));
    }

    #[test]
    fn result_model_usage_prefers_tracked_assistant_model() {
        let rows = vec![
            r#"{"type":"assistant","message":{"role":"assistant","model":"claude-main","content":[{"type":"text","text":"Done."}]}}"#.to_string(),
            r#"{"type":"result","subtype":"success","usage":{"input_tokens":10,"output_tokens":5},"modelUsage":{"claude-subagent":{"contextWindow":100000,"maxOutputTokens":16000},"claude-main":{"contextWindow":200000,"maxOutputTokens":32000}}}"#.to_string(),
        ];

        let entries = transcript_rows_to_entries(&rows, 50000);
        let usage = entries
            .iter()
            .find_map(|entry| match &entry.entry_type {
                NormalizedEntryType::TokenUsageInfo(usage) => Some(usage),
                _ => None,
            })
            .expect("expected token usage");

        assert_eq!(usage.model_context_window, 200000);
        assert_eq!(usage.max_output_tokens, Some(32000));
    }

    #[test]
    fn lowercase_alias_tool_names_use_canonical_label_and_content() {
        let rows = vec![
            r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","id":"toolu_1","name":"bash","input":{"command":"echo hi"}}]}}"#.to_string(),
        ];

        let entries = transcript_rows_to_entries(&rows, 200000);
        let NormalizedEntryType::ToolUse {
            tool_name,
            action_type,
            ..
        } = &entries[0].entry_type
        else {
            panic!("expected tool use entry");
        };

        assert_eq!(tool_name, "Bash");
        assert_eq!(entries[0].content, "echo hi");
        assert!(matches!(
            action_type,
            ActionType::CommandRun { command, .. } if command == "echo hi"
        ));
    }

    #[tokio::test]
    async fn transcript_delta_keeps_partial_jsonl_tail_for_next_hook() {
        let tempdir = tempfile::tempdir().unwrap();
        let transcript_path = tempdir.path().join("transcript.jsonl");
        let store = std::sync::Arc::new(utils::msg_store::MsgStore::new());
        let mut state = ClaudeTerminalHookState::default();
        let payload: ClaudeHookPayload = serde_json::from_value(serde_json::json!({
            "session_id": "claude-session-123",
            "hook_event_name": "PostToolUse",
            "cwd": "/tmp"
        }))
        .unwrap();

        tokio::fs::write(
            &transcript_path,
            r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"partial"#,
        )
        .await
        .unwrap();
        ingest_transcript_delta(
            &store,
            &mut state,
            &payload,
            transcript_path.to_str().unwrap(),
        )
        .await
        .unwrap();
        assert_eq!(state.entry_count, 0);

        tokio::fs::write(
            &transcript_path,
            r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"partial"}]}}"#,
        )
        .await
        .unwrap();
        ingest_transcript_delta(
            &store,
            &mut state,
            &payload,
            transcript_path.to_str().unwrap(),
        )
        .await
        .unwrap();

        assert_eq!(state.entry_count, 1);
    }

    #[tokio::test]
    async fn transcript_delta_returns_only_consumed_offset_for_partial_tail() {
        let tempdir = tempfile::tempdir().unwrap();
        let transcript_path = tempdir.path().join("transcript.jsonl");
        let store = std::sync::Arc::new(utils::msg_store::MsgStore::new());
        let mut state = ClaudeTerminalHookState::default();
        let payload: ClaudeHookPayload = serde_json::from_value(serde_json::json!({
            "session_id": "claude-session-123",
            "hook_event_name": "PostToolUse",
            "cwd": "/tmp"
        }))
        .unwrap();
        let complete_row = concat!(
            r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"done"}]}}"#,
            "\n"
        );
        let partial_row =
            r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text""#;

        tokio::fs::write(&transcript_path, format!("{complete_row}{partial_row}"))
            .await
            .unwrap();
        let offset = ingest_transcript_delta(
            &store,
            &mut state,
            &payload,
            transcript_path.to_str().unwrap(),
        )
        .await
        .unwrap();

        assert_eq!(offset, complete_row.len());
        assert_eq!(
            state.transcript_bytes_read,
            complete_row.len() + partial_row.len()
        );
        assert_eq!(state.entry_count, 1);

        let still_partial = format!("{complete_row}{partial_row}still-partial");
        tokio::fs::write(&transcript_path, &still_partial)
            .await
            .unwrap();
        let offset = ingest_transcript_delta(
            &store,
            &mut state,
            &payload,
            transcript_path.to_str().unwrap(),
        )
        .await
        .unwrap();

        assert_eq!(offset, complete_row.len());
        assert_eq!(state.transcript_bytes_read, still_partial.len());
        assert_eq!(state.entry_count, 1);
    }

    #[tokio::test]
    async fn transcript_delta_preserves_tool_state_across_hooks() {
        let tempdir = tempfile::tempdir().unwrap();
        let transcript_path = tempdir.path().join("transcript.jsonl");
        let store = std::sync::Arc::new(utils::msg_store::MsgStore::new());
        let mut state = ClaudeTerminalHookState::default();
        let payload: ClaudeHookPayload = serde_json::from_value(serde_json::json!({
            "session_id": "claude-session-123",
            "hook_event_name": "PostToolUse",
            "cwd": "/tmp"
        }))
        .unwrap();

        tokio::fs::write(
            &transcript_path,
            concat!(
                r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","id":"toolu_1","name":"Bash","input":{"command":"false"}}]}}"#,
                "\n"
            ),
        )
        .await
        .unwrap();
        ingest_transcript_delta(
            &store,
            &mut state,
            &payload,
            transcript_path.to_str().unwrap(),
        )
        .await
        .unwrap();

        tokio::fs::write(
            &transcript_path,
            concat!(
                r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","id":"toolu_1","name":"Bash","input":{"command":"false"}}]}}"#,
                "\n",
                r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_1","content":"{\"output\":\"failed\",\"exitCode\":1}"}]}}"#,
                "\n"
            ),
        )
        .await
        .unwrap();
        ingest_transcript_delta(
            &store,
            &mut state,
            &payload,
            transcript_path.to_str().unwrap(),
        )
        .await
        .unwrap();

        let history = store.get_history();
        assert_eq!(history.len(), 2);
        assert!(matches!(
            &history[1],
            utils::log_msg::LogMsg::JsonPatch(patch)
                if serde_json::to_value(patch).unwrap()[0]["op"] == "replace"
        ));
    }

    #[test]
    fn generic_tool_result_preserves_json_payloads() {
        let rows = vec![
            r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","id":"toolu_1","name":"mcp__server__lookup","input":{"id":"123"}}]}}"#.to_string(),
            r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_1","content":"{\"ok\":true}"}]}}"#.to_string(),
        ];

        let entries = transcript_rows_to_entries(&rows, 200000);
        let NormalizedEntryType::ToolUse { action_type, .. } = &entries[0].entry_type else {
            panic!("expected tool use entry");
        };
        let ActionType::Tool { result, .. } = action_type else {
            panic!("expected generic tool action");
        };
        let result = result.as_ref().expect("expected result");
        assert!(matches!(result.r#type, ToolResultValueType::Json));
        assert_eq!(result.value, serde_json::json!({"ok": true}));
    }

    #[test]
    fn imports_file_edit_and_write_changes() {
        let rows = vec![
            r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","id":"toolu_1","name":"Edit","input":{"file_path":"/repo/src/lib.rs","old_string":"old","new_string":"new"}},{"type":"tool_use","id":"toolu_2","name":"Write","input":{"file_path":"/repo/src/new.rs","content":"hello"}}]}}"#.to_string(),
        ];

        let entries = transcript_rows_to_entries_in_worktree(&rows, 200000, "/repo");

        let NormalizedEntryType::ToolUse {
            action_type: edit_action,
            ..
        } = &entries[0].entry_type
        else {
            panic!("expected edit tool");
        };
        assert!(matches!(
            edit_action,
            ActionType::FileEdit { path, changes } if path == "src/lib.rs"
                && matches!(changes.first(), Some(FileChange::Edit { unified_diff, .. }) if unified_diff.contains("-old") && unified_diff.contains("+new"))
        ));

        let NormalizedEntryType::ToolUse {
            action_type: write_action,
            ..
        } = &entries[1].entry_type
        else {
            panic!("expected write tool");
        };
        assert!(matches!(
            write_action,
            ActionType::FileEdit { path, changes } if path == "src/new.rs"
                && matches!(changes.first(), Some(FileChange::Write { content }) if content == "hello")
        ));
    }

    #[test]
    fn imports_todo_plan_question_and_web_search_tool_shapes() {
        let rows = vec![
            r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","id":"toolu_1","name":"TodoWrite","input":{"todos":[{"content":"ship it","status":"in_progress","priority":"high"}]}},{"type":"tool_use","id":"toolu_2","name":"ExitPlanMode","input":{"plan":"do it"}},{"type":"tool_use","id":"toolu_3","name":"AskUserQuestion","input":{"questions":[{"question":"Proceed?","header":"Confirm","multiSelect":false,"options":[{"label":"Yes","description":"Continue"}]}]}},{"type":"tool_use","id":"toolu_4","name":"web_search","input":{"query":"Claude hooks"}}]}}"#.to_string(),
        ];

        let entries = transcript_rows_to_entries(&rows, 200000);

        assert!(matches!(
            &entries[0].entry_type,
            NormalizedEntryType::ToolUse {
                action_type: ActionType::TodoManagement { todos, operation },
                ..
            } if operation == "write" && todos.len() == 1 && todos[0].content == "ship it"
        ));
        assert!(matches!(
            &entries[1].entry_type,
            NormalizedEntryType::ToolUse {
                action_type: ActionType::PlanPresentation { plan },
                ..
            } if plan == "do it"
        ));
        assert!(matches!(
            &entries[2].entry_type,
            NormalizedEntryType::ToolUse {
                action_type: ActionType::AskUserQuestion { questions },
                ..
            } if questions.len() == 1 && questions[0].question == "Proceed?"
        ));
        assert!(matches!(
            &entries[3].entry_type,
            NormalizedEntryType::ToolUse {
                action_type: ActionType::WebFetch { url },
                ..
            } if url == "Claude hooks"
        ));
    }
}
