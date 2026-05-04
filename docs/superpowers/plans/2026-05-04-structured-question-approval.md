# Structured Question Approval Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Thread `AskUserQuestion` structured data (header, options, multi-select) through the entire approval stack so the UI can render proper question forms, and fix the answer-key bug and `permission_denials` gap.

**Architecture:** `AskUserQuestionItem`/`AskUserQuestionOption` types already exist in `logs/mod.rs` — we reuse them rather than creating duplicates. Questions flow from Claude's `tool_input` → `create_question_approval(questions)` → `ApprovalInfo.questions` → WebSocket → frontend. A new `QuestionForm` component renders option-selection UI; the existing approve/deny UI is retained for non-question tool approvals.

**Tech Stack:** Rust (axum, serde, async-trait, ts-rs), React + TypeScript (Vite, Tailwind), pnpm

---

## What already works (no changes needed)

- Bidirectional `control_response` / `ControlResponseMessage` protocol
- Plan mode: `ExitPlanMode` → `handle_approval` → `PermissionResult::Allow { updated_permissions: [SetMode(BypassPermissions)] }`
- Tool approve/deny flow end-to-end
- `AskUserQuestion` routing in `on_can_use_tool` → `handle_question`
- `ClaudeJson::Result` detection + `turn_idle_tx`

## What is broken / missing

1. **Answer-key bug**: `handle_question` keys answers by `qa.question` (text); Claude Code CLI expects the `header` field as key.
2. **`QuestionAnswer` missing `header`**: the struct has no `header` field, so the correct key cannot be stored or submitted from the frontend.
3. **Questions not passed to UI**: `create_question_approval` accepts `question_count: usize` — the structured question data is discarded before reaching `ApprovalInfo`.
4. **No question UI**: `PendingApprovalEntry.tsx` shows approve/deny for everything; questions need an option-selection form.
5. **`permission_denials` absent from `ClaudeJson::Result`**: blocked tool names are silently dropped.

---

## File Map

| Action | Path | Responsibility |
|--------|------|----------------|
| Modify | `crates/utils/src/approvals.rs` | Add `header` field to `QuestionAnswer` |
| Modify | `crates/executors/src/approvals.rs` | Change trait to accept `&[AskUserQuestionItem]` |
| Modify | `crates/services/src/services/approvals.rs` | Add `questions` to `PendingApproval` + `ApprovalInfo` |
| Modify | `crates/services/src/services/approvals/executor_approvals.rs` | Thread questions through bridge |
| Modify | `crates/executors/src/executors/claude/client.rs` | Parse questions from input; fix answer key |
| Modify | `crates/executors/src/executors/claude.rs` | Add `permission_denials` to `ClaudeJson::Result` |
| Modify | `crates/server/src/bin/generate_types.rs` | Export `ApprovalInfo` (already present; verify) |
| Create | `packages/web-core/src/shared/components/NormalizedConversation/QuestionForm.tsx` | Option-selection form for AskUserQuestion |
| Modify | `packages/web-core/src/shared/components/NormalizedConversation/PendingApprovalEntry.tsx` | Branch on `is_question` to show `QuestionForm` |

---

## Task 1: Add `header` to `QuestionAnswer`

**Files:**
- Modify: `crates/utils/src/approvals.rs`

- [ ] **Step 1: Write the failing test**

In `crates/utils/src/approvals.rs`, add a test at the bottom of the file:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn question_answer_roundtrips_with_header() {
        let qa = QuestionAnswer {
            question: "Which colour?".to_string(),
            header: "colour".to_string(),
            answer: vec!["Red".to_string()],
        };
        let json = serde_json::to_string(&qa).unwrap();
        let back: QuestionAnswer = serde_json::from_str(&json).unwrap();
        assert_eq!(back.header, "colour");
        assert_eq!(back.answer, vec!["Red".to_string()]);
    }
}
```

- [ ] **Step 2: Run test to confirm it fails**

```bash
cd /Users/david/Code/.vibe-kanban-workspaces/c8fb-does-vibe-kanban/vibe-kanban
cargo test -p workspace-utils approvals::tests::question_answer_roundtrips_with_header 2>&1 | tail -20
```

Expected: compile error — `QuestionAnswer` has no field `header`.

- [ ] **Step 3: Add `header` to `QuestionAnswer`**

In `crates/utils/src/approvals.rs`, change:

```rust
/// A question–answer pair. `answer` holds one or more selected labels/values.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct QuestionAnswer {
    pub question: String,
    pub answer: Vec<String>,
}
```

to:

```rust
/// A question–answer pair. `answer` holds one or more selected labels/values.
/// `header` is the key Claude Code CLI uses to match answers to questions.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct QuestionAnswer {
    pub question: String,
    pub header: String,
    pub answer: Vec<String>,
}
```

- [ ] **Step 4: Run the test**

```bash
cargo test -p workspace-utils approvals::tests::question_answer_roundtrips_with_header 2>&1 | tail -10
```

Expected: `test result: ok. 1 passed`.

- [ ] **Step 5: Commit**

```bash
git add crates/utils/src/approvals.rs
git commit -m "feat: add header field to QuestionAnswer for correct Claude Code CLI answer keying"
```

---

## Task 2: Update `ExecutorApprovalService` trait to accept structured questions

**Files:**
- Modify: `crates/executors/src/approvals.rs`

- [ ] **Step 1: Write the failing test**

Add to `crates/executors/src/approvals.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::logs::{AskUserQuestionItem, AskUserQuestionOption};

    #[tokio::test]
    async fn noop_service_question_approval_returns_service_unavailable() {
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
        assert!(result.is_ok()); // noop returns "noop"
    }
}
```

- [ ] **Step 2: Run test to confirm it fails**

```bash
cargo test -p executors approvals::tests::noop_service_question_approval_returns_service_unavailable 2>&1 | tail -20
```

Expected: compile error — `create_question_approval` still takes `question_count: usize`.

- [ ] **Step 3: Update the trait and noop impl**

Replace the `ExecutorApprovalService` trait in `crates/executors/src/approvals.rs`:

```rust
use crate::logs::AskUserQuestionItem;
```

Add this import at the top, then change:

```rust
    /// Creates a question approval request. Returns the approval_id immediately.
    async fn create_question_approval(
        &self,
        tool_name: &str,
        question_count: usize,
    ) -> Result<String, ExecutorApprovalError>;
```

to:

```rust
    /// Creates a question approval request. Returns the approval_id immediately.
    async fn create_question_approval(
        &self,
        tool_name: &str,
        questions: &[AskUserQuestionItem],
    ) -> Result<String, ExecutorApprovalError>;
```

Update the `NoopExecutorApprovalService` impl to match:

```rust
    async fn create_question_approval(
        &self,
        _tool_name: &str,
        _questions: &[AskUserQuestionItem],
    ) -> Result<String, ExecutorApprovalError> {
        Ok("noop".to_string())
    }
```

- [ ] **Step 4: Run the test**

```bash
cargo test -p executors approvals::tests::noop_service_question_approval_returns_service_unavailable 2>&1 | tail -10
```

Expected: `test result: ok. 1 passed`.

- [ ] **Step 5: Commit**

```bash
git add crates/executors/src/approvals.rs
git commit -m "feat: update ExecutorApprovalService to accept structured questions instead of question_count"
```

---

## Task 3: Thread questions through `ApprovalInfo` and the service layer

**Files:**
- Modify: `crates/services/src/services/approvals.rs`

- [ ] **Step 1: Write the failing test**

Add to `crates/services/src/services/approvals.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use executors::logs::{AskUserQuestionItem, AskUserQuestionOption};
    use utils::approvals::ApprovalRequest;
    use uuid::Uuid;

    #[tokio::test]
    async fn approval_info_carries_questions() {
        let svc = Approvals::new();
        let request = ApprovalRequest::new("AskUserQuestion".to_string(), Uuid::new_v4());
        let questions = vec![AskUserQuestionItem {
            question: "Pick one".to_string(),
            header: "pick".to_string(),
            options: vec![AskUserQuestionOption {
                label: "A".to_string(),
                description: "Option A".to_string(),
            }],
            multi_select: false,
        }];
        let (_, _waiter) = svc
            .create_with_waiter(request, true, Some(questions.clone()))
            .await
            .unwrap();

        let infos = svc.pending_infos();
        assert_eq!(infos.len(), 1);
        let q = infos[0].questions.as_ref().unwrap();
        assert_eq!(q[0].header, "pick");
    }
}
```

- [ ] **Step 2: Run test to confirm it fails**

```bash
cargo test -p services approvals::tests::approval_info_carries_questions 2>&1 | tail -20
```

Expected: compile error — `create_with_waiter` has wrong signature, `ApprovalInfo` has no `questions`.

- [ ] **Step 3: Update `PendingApproval`, `ApprovalInfo`, and `create_with_waiter`**

In `crates/services/src/services/approvals.rs`, add the import at the top:

```rust
use executors::logs::AskUserQuestionItem;
```

Update `PendingApproval` (private struct):

```rust
#[derive(Debug)]
struct PendingApproval {
    execution_process_id: Uuid,
    tool_name: String,
    is_question: bool,
    created_at: DateTime<Utc>,
    timeout_at: DateTime<Utc>,
    response_tx: oneshot::Sender<ApprovalOutcome>,
    questions: Option<Vec<AskUserQuestionItem>>,  // ADD
}
```

Update `ApprovalInfo` (public, exported to TS):

```rust
/// Info about a currently pending approval, sent to the frontend via WebSocket.
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
pub struct ApprovalInfo {
    pub approval_id: String,
    pub tool_name: String,
    pub execution_process_id: Uuid,
    pub is_question: bool,
    pub created_at: DateTime<Utc>,
    pub timeout_at: DateTime<Utc>,
    pub questions: Option<Vec<AskUserQuestionItem>>,  // ADD
}
```

Update `create_with_waiter` signature and body:

```rust
    pub(crate) async fn create_with_waiter(
        &self,
        request: ApprovalRequest,
        is_question: bool,
        questions: Option<Vec<AskUserQuestionItem>>,  // ADD
    ) -> Result<(ApprovalRequest, ApprovalWaiter), ApprovalError> {
        let (tx, rx) = oneshot::channel();
        let default_timeout = ApprovalOutcome::TimedOut;
        let waiter: ApprovalWaiter = rx
            .map(move |result| result.unwrap_or(default_timeout))
            .boxed()
            .shared();
        let req_id = request.id.clone();

        let info = ApprovalInfo {
            approval_id: req_id.clone(),
            tool_name: request.tool_name.clone(),
            execution_process_id: request.execution_process_id,
            is_question,
            created_at: request.created_at,
            timeout_at: request.timeout_at,
            questions: questions.clone(),  // ADD
        };

        let pending_approval = PendingApproval {
            execution_process_id: request.execution_process_id,
            tool_name: request.tool_name.clone(),
            is_question,
            created_at: request.created_at,
            timeout_at: request.timeout_at,
            response_tx: tx,
            questions,  // ADD
        };

        self.pending.insert(req_id.clone(), pending_approval);
        let _ = self
            .patches_tx
            .send(crate::services::events::patches::approvals_patch::created(&info));
        self.spawn_timeout_watcher(req_id.clone(), request.timeout_at, waiter.clone());
        Ok((request, waiter))
    }
```

Update `pending_infos()` to include questions:

```rust
    fn pending_infos(&self) -> Vec<ApprovalInfo> {
        self.pending
            .iter()
            .map(|entry| {
                let p = entry.value();
                ApprovalInfo {
                    approval_id: entry.key().clone(),
                    tool_name: p.tool_name.clone(),
                    execution_process_id: p.execution_process_id,
                    is_question: p.is_question,
                    created_at: p.created_at,
                    timeout_at: p.timeout_at,
                    questions: p.questions.clone(),  // ADD
                }
            })
            .collect()
    }
```

Also update the non-question call in `Approvals::cancel` — `create_with_waiter` is only called from `ExecutorApprovalBridge` so there's nothing else to update here.

- [ ] **Step 4: Run the test**

```bash
cargo test -p services approvals::tests::approval_info_carries_questions 2>&1 | tail -10
```

Expected: `test result: ok. 1 passed`.

- [ ] **Step 5: Commit**

```bash
git add crates/services/src/services/approvals.rs
git commit -m "feat: thread structured questions through ApprovalInfo to frontend"
```

---

## Task 4: Update `ExecutorApprovalBridge` to pass questions

**Files:**
- Modify: `crates/services/src/services/approvals/executor_approvals.rs`

- [ ] **Step 1: Run `cargo check` to see compile errors**

```bash
cargo check -p services 2>&1 | grep "error\[" | head -20
```

Expected: errors about wrong argument count in `create_with_waiter` calls and signature mismatch in `create_question_approval`.

- [ ] **Step 2: Update `create_internal` and `create_question_approval`**

Add the import at the top of `crates/services/src/services/approvals/executor_approvals.rs`:

```rust
use executors::logs::AskUserQuestionItem;
```

Update `create_internal` to accept optional questions:

```rust
    async fn create_internal(
        &self,
        tool_name: &str,
        is_question: bool,
        questions: Option<&[AskUserQuestionItem]>,  // CHANGED from question_count: Option<usize>
    ) -> Result<String, ExecutorApprovalError> {
        let request = ApprovalRequest::new(tool_name.to_string(), self.execution_process_id);

        let (request, waiter) = self
            .approvals
            .create_with_waiter(request, is_question, questions.map(|q| q.to_vec()))  // pass questions
            .await
            .map_err(ExecutorApprovalError::request_failed)?;

        let approval_id = request.id.clone();
        self.waiters.lock().await.insert(approval_id.clone(), waiter);

        let (workspace_name, workspace_id) =
            ExecutionProcess::load_context(&self.db.pool, self.execution_process_id)
                .await
                .map(|ctx| {
                    let name = ctx
                        .workspace
                        .name
                        .unwrap_or_else(|| ctx.workspace.branch.clone());
                    (name, Some(ctx.workspace.id))
                })
                .unwrap_or_else(|_| ("Unknown workspace".to_string(), None));

        let (title, message) = if is_question {
            let count = questions.map(|q| q.len()).unwrap_or(1);
            if count == 1 {
                (
                    format!("Question Asked: {workspace_name}"),
                    "1 question requires an answer".to_string(),
                )
            } else {
                (
                    format!("Question Asked: {workspace_name}"),
                    format!("{count} questions require answers"),
                )
            }
        } else {
            (
                format!("Approval Needed: {workspace_name}"),
                format!("Tool '{tool_name}' requires approval"),
            )
        };

        self.notification_service.notify(&title, &message, workspace_id).await;
        Ok(approval_id)
    }
```

Update `create_tool_approval`:

```rust
    async fn create_tool_approval(&self, tool_name: &str) -> Result<String, ExecutorApprovalError> {
        self.create_internal(tool_name, false, None).await
    }
```

Update `create_question_approval` to new signature:

```rust
    async fn create_question_approval(
        &self,
        tool_name: &str,
        questions: &[AskUserQuestionItem],
    ) -> Result<String, ExecutorApprovalError> {
        self.create_internal(tool_name, true, Some(questions)).await
    }
```

- [ ] **Step 3: Run `cargo check -p services`**

```bash
cargo check -p services 2>&1 | grep "error\[" | head -20
```

Expected: no errors.

- [ ] **Step 4: Run all tests**

```bash
cargo test -p services 2>&1 | tail -15
```

Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add crates/services/src/services/approvals/executor_approvals.rs
git commit -m "feat: pass structured questions from executor bridge to approval service"
```

---

## Task 5: Fix answer-key bug and parse questions in `client.rs`

**Files:**
- Modify: `crates/executors/src/executors/claude/client.rs`

- [ ] **Step 1: Write the failing test**

Add to `crates/executors/src/executors/claude/client.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn answer_map_uses_header_as_key() {
        // Simulate building the answer map the same way handle_question does
        let answers = vec![
            utils::approvals::QuestionAnswer {
                question: "Which colour?".to_string(),
                header: "colour".to_string(),
                answer: vec!["Red".to_string()],
            },
        ];
        let map: serde_json::Map<String, serde_json::Value> = answers
            .iter()
            .map(|qa| {
                (
                    qa.header.clone(),
                    serde_json::Value::String(qa.answer.join(", ")),
                )
            })
            .collect();
        assert!(map.contains_key("colour"), "key must be header, not question text");
        assert!(!map.contains_key("Which colour?"), "must not use question text as key");
    }
}
```

- [ ] **Step 2: Run test to confirm it fails**

```bash
cargo test -p executors claude::client::tests::answer_map_uses_header_as_key 2>&1 | tail -20
```

Expected: compile error — `QuestionAnswer` has no `header` field yet in the type check, or test is already failing due to the old key logic. (If Task 1 is done, this will compile; the assertion about `"colour"` key would pass only after fixing the production code.)

- [ ] **Step 3: Update `handle_question` to parse questions and fix answer key**

In `crates/executors/src/executors/claude/client.rs`, replace the entire `handle_question` method:

```rust
    async fn handle_question(
        &self,
        tool_use_id: String,
        tool_name: String,
        tool_input: serde_json::Value,
    ) -> Result<PermissionResult, ExecutorError> {
        let approval_service = self
            .approvals
            .as_ref()
            .ok_or(ExecutorApprovalError::ServiceUnavailable)?;

        // Parse structured questions from tool input so they flow to the UI.
        let questions: Vec<crate::logs::AskUserQuestionItem> = tool_input
            .get("questions")
            .and_then(|q| serde_json::from_value(q.clone()).ok())
            .unwrap_or_default();

        let approval_id = match approval_service
            .create_question_approval(&tool_name, &questions)
            .await
        {
            Ok(id) => id,
            Err(err) => {
                self.handle_question_error(&tool_use_id, &tool_name, &err)
                    .await?;
                return Err(err.into());
            }
        };

        let _ = self
            .log_writer
            .log_raw(&serde_json::to_string(&ClaudeJson::ApprovalRequested {
                tool_call_id: tool_use_id.clone(),
                tool_name: tool_name.clone(),
                approval_id: approval_id.clone(),
            })?)
            .await;

        let status = match approval_service
            .wait_question_answer(&approval_id, self.cancel.clone())
            .await
        {
            Ok(s) => s,
            Err(err) => {
                self.handle_question_error(&tool_use_id, &tool_name, &err)
                    .await?;
                return Err(err.into());
            }
        };

        self.log_writer
            .log_raw(&serde_json::to_string(&ClaudeJson::QuestionResponse {
                tool_call_id: tool_use_id.clone(),
                tool_name: tool_name.clone(),
                question_status: status.clone(),
            })?)
            .await?;

        match status {
            QuestionStatus::Answered { answers } => {
                // Claude Code CLI expects answers keyed by `header`, not question text.
                let answers_map: serde_json::Map<String, serde_json::Value> = answers
                    .iter()
                    .map(|qa| {
                        (
                            qa.header.clone(),
                            serde_json::Value::String(qa.answer.join(", ")),
                        )
                    })
                    .collect();
                let mut updated = tool_input.clone();
                if let Some(obj) = updated.as_object_mut() {
                    obj.insert(
                        "answers".to_string(),
                        serde_json::Value::Object(answers_map),
                    );
                }
                Ok(PermissionResult::Allow {
                    updated_input: updated,
                    updated_permissions: None,
                })
            }
            QuestionStatus::TimedOut => Ok(PermissionResult::Deny {
                message: "Question request timed out".to_string(),
                interrupt: Some(true),
            }),
        }
    }
```

- [ ] **Step 4: Run tests**

```bash
cargo test -p executors claude::client::tests::answer_map_uses_header_as_key 2>&1 | tail -10
```

Expected: `test result: ok. 1 passed`.

- [ ] **Step 5: Full workspace check**

```bash
cargo check --workspace 2>&1 | grep "error\[" | head -20
```

Expected: no errors.

- [ ] **Step 6: Commit**

```bash
git add crates/executors/src/executors/claude/client.rs
git commit -m "fix: use question header as answer key in AskUserQuestion response; pass structured questions to approval service"
```

---

## Task 6: Add `permission_denials` to `ClaudeJson::Result`

**Files:**
- Modify: `crates/executors/src/executors/claude.rs`

- [ ] **Step 1: Write the failing test**

Search for the `ClaudeJson` deserialize tests in `claude.rs` or add a new one:

```rust
#[test]
fn result_message_deserializes_permission_denials() {
    let json = r#"{
        "type": "result",
        "subtype": "success",
        "is_error": false,
        "permissionDenials": [
            {"toolName": "Bash"},
            {"toolName": "Edit"}
        ]
    }"#;
    let msg: ClaudeJson = serde_json::from_str(json).unwrap();
    if let ClaudeJson::Result { permission_denials, .. } = msg {
        let denials = permission_denials.unwrap();
        assert_eq!(denials.len(), 2);
    } else {
        panic!("expected ClaudeJson::Result");
    }
}
```

Find where `ClaudeJson` tests live:

```bash
grep -n "#\[cfg(test)\]" /Users/david/Code/.vibe-kanban-workspaces/c8fb-does-vibe-kanban/vibe-kanban/crates/executors/src/executors/claude.rs | head -5
```

Add the test to that module, or to a new `#[cfg(test)] mod tests { ... }` block at the bottom of the file.

- [ ] **Step 2: Run test to confirm it fails**

```bash
cargo test -p executors claude::tests::result_message_deserializes_permission_denials 2>&1 | tail -10
```

Expected: `ClaudeJson::Result` has no `permission_denials` field — compile or runtime failure.

- [ ] **Step 3: Add `permission_denials` to the `Result` variant**

In `crates/executors/src/executors/claude.rs`, find the `ClaudeJson::Result` variant (around line 2355) and add the field:

```rust
    Result {
        #[serde(default)]
        subtype: Option<String>,
        #[serde(default, alias = "isError")]
        is_error: Option<bool>,
        #[serde(default, alias = "durationMs")]
        duration_ms: Option<u64>,
        #[serde(default)]
        result: Option<serde_json::Value>,
        #[serde(default)]
        error: Option<String>,
        #[serde(default, alias = "numTurns")]
        num_turns: Option<u32>,
        #[serde(default, alias = "sessionId")]
        session_id: Option<String>,
        #[serde(default, alias = "modelUsage")]
        model_usage: Option<HashMap<String, ClaudeModelUsage>>,
        #[serde(default)]
        usage: Option<ClaudeUsage>,
        #[serde(default, alias = "permissionDenials")]
        permission_denials: Option<Vec<serde_json::Value>>,  // ADD
    },
```

- [ ] **Step 4: Handle `permission_denials` in the `ClaudeJson::Result` match arm**

Find the `ClaudeJson::Result { is_error, model_usage, subtype, result, .. }` match arm (around line 1826) and update the destructuring and handler:

```rust
            ClaudeJson::Result {
                is_error,
                model_usage,
                subtype,
                result,
                permission_denials,  // ADD
                ..
            } => {
                // existing context-window / token-usage logic unchanged ...
                if let Some(context_window) = model_usage.as_ref().and_then(|model_usage| {
                    self.main_model_name
                        .as_ref()
                        .and_then(|name| model_usage.get(name))
                        .and_then(|usage| usage.context_window)
                }) {
                    self.main_model_context_window = context_window;
                    patches.push(self.add_token_usage_entry(entry_index_provider));
                }

                // existing error / success result entry logic unchanged ...
                if matches!(self.strategy, HistoryStrategy::AmpResume) && is_error.unwrap_or(false)
                {
                    let entry = NormalizedEntry {
                        timestamp: None,
                        entry_type: NormalizedEntryType::ErrorMessage {
                            error_type: NormalizedEntryError::Other,
                        },
                        content: serde_json::to_string(claude_json)
                            .unwrap_or_else(|_| "error".to_string()),
                        metadata: Some(
                            serde_json::to_value(claude_json).unwrap_or(serde_json::Value::Null),
                        ),
                    };
                    let idx = entry_index_provider.next();
                    patches.push(ConversationPatch::add_normalized_entry(idx, entry));
                } else if matches!(subtype.as_deref(), Some("success"))
                    && let Some(text) = result.as_ref().and_then(|v| v.as_str())
                    && (self.last_assistant_message.is_none()
                        || matches!(&self.last_assistant_message, Some(message) if !message.contains(text)))
                {
                    let entry = NormalizedEntry {
                        timestamp: None,
                        entry_type: NormalizedEntryType::AssistantMessage,
                        content: text.to_string(),
                        metadata: Some(
                            serde_json::to_value(claude_json).unwrap_or(serde_json::Value::Null),
                        ),
                    };
                    let idx = entry_index_provider.next();
                    patches.push(ConversationPatch::add_normalized_entry(idx, entry));
                }

                // ADD: emit a system message listing blocked tools
                if let Some(denials) = permission_denials.as_ref().filter(|d| !d.is_empty()) {
                    let tool_names: Vec<String> = denials
                        .iter()
                        .filter_map(|d| {
                            d.get("toolName")
                                .or_else(|| d.get("tool_name"))
                                .and_then(|v| v.as_str())
                                .map(String::from)
                        })
                        .collect();
                    if !tool_names.is_empty() {
                        let content = format!(
                            "Tools blocked by permissions: {} ({} denial(s))",
                            tool_names.join(", "),
                            denials.len()
                        );
                        let entry = NormalizedEntry {
                            timestamp: None,
                            entry_type: NormalizedEntryType::SystemMessage,
                            content,
                            metadata: None,
                        };
                        let idx = entry_index_provider.next();
                        patches.push(ConversationPatch::add_normalized_entry(idx, entry));
                    }
                }
            }
```

- [ ] **Step 5: Run test**

```bash
cargo test -p executors claude::tests::result_message_deserializes_permission_denials 2>&1 | tail -10
```

Expected: `test result: ok. 1 passed`.

- [ ] **Step 6: Full workspace compile check**

```bash
cargo check --workspace 2>&1 | grep "error\[" | head -20
```

Expected: no errors.

- [ ] **Step 7: Commit**

```bash
git add crates/executors/src/executors/claude.rs
git commit -m "feat: capture permission_denials from ClaudeJson::Result and surface as system message"
```

---

## Task 7: Regenerate TypeScript types

**Files:**
- Run: `pnpm run generate-types`
- Verify: `shared/types.ts` has updated `QuestionAnswer`, `ApprovalInfo`

- [ ] **Step 1: Run type generation**

```bash
cd /Users/david/Code/.vibe-kanban-workspaces/c8fb-does-vibe-kanban/vibe-kanban
pnpm run generate-types 2>&1 | tail -20
```

Expected: exits 0. If `AskUserQuestionItem` is referenced by `ApprovalInfo` but not yet listed in `generate_types.rs`, you'll get a missing-type error.

- [ ] **Step 2: Verify `generate_types.rs` exports `ApprovalInfo`**

```bash
grep "ApprovalInfo" crates/server/src/bin/generate_types.rs
```

Expected: `services::services::approvals::ApprovalInfo::decl()` appears. If not, add it.

`AskUserQuestionItem` is already exported (line 263 of generate_types.rs), so `ApprovalInfo.questions: Option<Vec<AskUserQuestionItem>>` will resolve correctly in TypeScript.

- [ ] **Step 3: Check generated types**

```bash
grep -A 8 "ApprovalInfo" shared/types.ts
grep -A 5 "QuestionAnswer" shared/types.ts
```

Expected output:
```typescript
export type ApprovalInfo = {
  approval_id: string;
  tool_name: string;
  execution_process_id: string;
  is_question: boolean;
  created_at: string;
  timeout_at: string;
  questions: Array<AskUserQuestionItem> | null;
};

export type QuestionAnswer = {
  question: string;
  header: string;
  answer: Array<string>;
};
```

- [ ] **Step 4: Run frontend type check**

```bash
pnpm run check 2>&1 | grep -E "error TS|Error" | head -20
```

Expected: only errors related to the `QuestionAnswer` and `ApprovalInfo` shape changes — these get fixed in Tasks 8 and 9.

- [ ] **Step 5: Commit**

```bash
git add shared/types.ts
git commit -m "chore: regenerate TypeScript types for QuestionAnswer.header and ApprovalInfo.questions"
```

---

## Task 8: Create `QuestionForm.tsx` component

**Files:**
- Create: `packages/web-core/src/shared/components/NormalizedConversation/QuestionForm.tsx`

- [ ] **Step 1: Create the component file**

```typescript
// packages/web-core/src/shared/components/NormalizedConversation/QuestionForm.tsx
import { useState, useCallback } from 'react';
import type { AskUserQuestionItem, QuestionAnswer } from 'shared/types';
import { Button } from '@vibe/ui/components/Button';

interface QuestionFormProps {
  questions: AskUserQuestionItem[];
  disabled: boolean;
  isResponding: boolean;
  onSubmit: (answers: QuestionAnswer[]) => void;
}

function SingleSelect({
  question,
  selectedLabel,
  onChange,
  disabled,
}: {
  question: AskUserQuestionItem;
  selectedLabel: string;
  onChange: (label: string) => void;
  disabled: boolean;
}) {
  return (
    <div className="flex flex-col gap-1.5">
      <p className="text-sm font-medium">{question.question}</p>
      <div className="flex flex-col gap-1">
        {question.options.map((opt) => (
          <label
            key={opt.label}
            className="flex cursor-pointer items-center gap-2 rounded-md px-2 py-1.5 text-sm hover:bg-muted/50"
          >
            <input
              type="radio"
              name={question.header}
              value={opt.label}
              checked={selectedLabel === opt.label}
              onChange={() => onChange(opt.label)}
              disabled={disabled}
              className="accent-primary"
            />
            <span className="font-medium">{opt.label}</span>
            {opt.description && (
              <span className="text-muted-foreground">— {opt.description}</span>
            )}
          </label>
        ))}
      </div>
    </div>
  );
}

function MultiSelect({
  question,
  selectedLabels,
  onChange,
  disabled,
}: {
  question: AskUserQuestionItem;
  selectedLabels: string[];
  onChange: (labels: string[]) => void;
  disabled: boolean;
}) {
  const toggle = useCallback(
    (label: string) => {
      onChange(
        selectedLabels.includes(label)
          ? selectedLabels.filter((l) => l !== label)
          : [...selectedLabels, label]
      );
    },
    [selectedLabels, onChange]
  );

  return (
    <div className="flex flex-col gap-1.5">
      <p className="text-sm font-medium">{question.question}</p>
      <div className="flex flex-col gap-1">
        {question.options.map((opt) => (
          <label
            key={opt.label}
            className="flex cursor-pointer items-center gap-2 rounded-md px-2 py-1.5 text-sm hover:bg-muted/50"
          >
            <input
              type="checkbox"
              value={opt.label}
              checked={selectedLabels.includes(opt.label)}
              onChange={() => toggle(opt.label)}
              disabled={disabled}
              className="accent-primary"
            />
            <span className="font-medium">{opt.label}</span>
            {opt.description && (
              <span className="text-muted-foreground">— {opt.description}</span>
            )}
          </label>
        ))}
      </div>
    </div>
  );
}

const QuestionForm = ({
  questions,
  disabled,
  isResponding,
  onSubmit,
}: QuestionFormProps) => {
  const [selections, setSelections] = useState<Record<string, string[]>>(() =>
    Object.fromEntries(questions.map((q) => [q.header, []]))
  );

  const updateSelection = useCallback((header: string, values: string[]) => {
    setSelections((prev) => ({ ...prev, [header]: values }));
  }, []);

  const allAnswered = questions.every(
    (q) => !q.multiSelect || selections[q.header]?.length > 0
  );

  const handleSubmit = useCallback(() => {
    const answers: QuestionAnswer[] = questions.map((q) => ({
      question: q.question,
      header: q.header,
      answer: selections[q.header] ?? [],
    }));
    onSubmit(answers);
  }, [questions, selections, onSubmit]);

  return (
    <div className="flex flex-col gap-4 px-4 py-3">
      {questions.map((q) =>
        q.multiSelect ? (
          <MultiSelect
            key={q.header}
            question={q}
            selectedLabels={selections[q.header] ?? []}
            onChange={(labels) => updateSelection(q.header, labels)}
            disabled={disabled}
          />
        ) : (
          <SingleSelect
            key={q.header}
            question={q}
            selectedLabel={selections[q.header]?.[0] ?? ''}
            onChange={(label) => updateSelection(q.header, [label])}
            disabled={disabled}
          />
        )
      )}
      <div className="flex justify-end">
        <Button
          size="sm"
          onClick={handleSubmit}
          disabled={disabled || isResponding || !allAnswered}
        >
          {isResponding ? 'Submitting…' : 'Submit'}
        </Button>
      </div>
    </div>
  );
};

export default QuestionForm;
```

- [ ] **Step 2: Run frontend type check on the new file**

```bash
cd /Users/david/Code/.vibe-kanban-workspaces/c8fb-does-vibe-kanban/vibe-kanban
pnpm run check 2>&1 | grep "QuestionForm" | head -10
```

Expected: no errors referencing `QuestionForm.tsx`.

- [ ] **Step 3: Commit**

```bash
git add packages/web-core/src/shared/components/NormalizedConversation/QuestionForm.tsx
git commit -m "feat: add QuestionForm component for AskUserQuestion option selection"
```

---

## Task 9: Update `PendingApprovalEntry.tsx` to branch on question vs tool approval

**Files:**
- Modify: `packages/web-core/src/shared/components/NormalizedConversation/PendingApprovalEntry.tsx`

- [ ] **Step 1: Update the approval response call and add question submission**

In `PendingApprovalEntry.tsx`, update the `respond` callback and add a `respondWithAnswers` callback. The key changes:

1. Import `QuestionForm` and `QuestionAnswer`
2. Add `respondWithAnswers` for question submissions (sends `ApprovalOutcome::Answered`)
3. Render `QuestionForm` when `approvalInfo?.is_question && approvalInfo?.questions`

Replace the file content with:

```typescript
import {
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
} from 'react';
import type { ReactNode } from 'react';
import type { ApprovalStatus, QuestionAnswer, ToolStatus } from 'shared/types';
import { Button } from '@vibe/ui/components/Button';
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from '@vibe/ui/components/RadixTooltip';
import { approvalsApi } from '@/shared/lib/api';
import { Check, X } from 'lucide-react';
import WYSIWYGEditor from '@/shared/components/WYSIWYGEditor';
import QuestionForm from './QuestionForm';

import { useHotkeysContext } from 'react-hotkeys-hook';
import { TabNavContext } from '@/shared/hooks/TabNavigationContext';
import {
  useKeyApproveRequest,
  useKeyDenyApproval,
  Scope,
} from '@/shared/keyboard';
import { useApprovalForm } from '@/shared/hooks/ApprovalForm';
import { useApprovals } from '@/shared/hooks/useApprovals';

const DEFAULT_DENIAL_REASON = 'User denied this tool use request.';

interface PendingApprovalEntryProps {
  pendingStatus: Extract<ToolStatus, { status: 'pending_approval' }>;
  executionProcessId?: string;
  children: ReactNode;
}

function useApprovalCountdown(
  requestedAt: string | number | Date,
  timeoutAt: string | number | Date,
  paused: boolean
) {
  const totalSeconds = useMemo(() => {
    const total = Math.floor(
      (new Date(timeoutAt).getTime() - new Date(requestedAt).getTime()) / 1000
    );
    return Math.max(1, total);
  }, [requestedAt, timeoutAt]);

  const [timeLeft, setTimeLeft] = useState<number>(() => {
    const remaining = new Date(timeoutAt).getTime() - Date.now();
    return Math.max(0, Math.floor(remaining / 1000));
  });

  useEffect(() => {
    if (paused) return;
    const id = window.setInterval(() => {
      const remaining = new Date(timeoutAt).getTime() - Date.now();
      const next = Math.max(0, Math.floor(remaining / 1000));
      setTimeLeft(next);
      if (next <= 0) window.clearInterval(id);
    }, 1000);
    return () => window.clearInterval(id);
  }, [timeoutAt, paused]);

  const percent = useMemo(
    () =>
      Math.max(0, Math.min(100, Math.round((timeLeft / totalSeconds) * 100))),
    [timeLeft, totalSeconds]
  );

  return { timeLeft, percent };
}

function ActionButtons({
  disabled,
  isResponding,
  onApprove,
  onStartDeny,
}: {
  disabled: boolean;
  isResponding: boolean;
  onApprove: () => void;
  onStartDeny: () => void;
}) {
  return (
    <div className="flex items-center gap-1.5 pr-4">
      <Tooltip>
        <TooltipTrigger asChild>
          <Button
            onClick={onApprove}
            variant="ghost"
            className="h-8 w-8 rounded-full p-0"
            disabled={disabled}
            aria-label={isResponding ? 'Submitting approval' : 'Approve'}
            aria-busy={isResponding}
          >
            <Check className="h-5 w-5" />
          </Button>
        </TooltipTrigger>
        <TooltipContent>
          <p>{isResponding ? 'Submitting…' : 'Approve request'}</p>
        </TooltipContent>
      </Tooltip>

      <Tooltip>
        <TooltipTrigger asChild>
          <Button
            onClick={onStartDeny}
            variant="ghost"
            className="h-8 w-8 rounded-full p-0"
            disabled={disabled}
            aria-label={isResponding ? 'Submitting denial' : 'Deny'}
            aria-busy={isResponding}
          >
            <X className="h-5 w-5" />
          </Button>
        </TooltipTrigger>
        <TooltipContent>
          <p>{isResponding ? 'Submitting…' : 'Provide denial reason'}</p>
        </TooltipContent>
      </Tooltip>
    </div>
  );
}

function DenyReasonForm({
  isResponding,
  value,
  onChange,
  onCancel,
  onSubmit,
}: {
  isResponding: boolean;
  value: string;
  onChange: (v: string) => void;
  onCancel: () => void;
  onSubmit: () => void;
}) {
  return (
    <div className="flex flex-col gap-2 p-4">
      <WYSIWYGEditor
        value={value}
        onChange={onChange}
        placeholder="Let the agent know why this request was denied... Type @ to insert tags or search files."
        disabled={isResponding}
        className="min-h-[80px]"
        onCmdEnter={onSubmit}
      />
      <div className="flex flex-wrap items-center justify-end gap-2">
        <Button
          variant="ghost"
          size="sm"
          onClick={onCancel}
          disabled={isResponding}
        >
          Cancel
        </Button>
        <Button size="sm" onClick={onSubmit} disabled={isResponding}>
          Deny
        </Button>
      </div>
    </div>
  );
}

const PendingApprovalEntry = ({
  pendingStatus,
  executionProcessId,
  children,
}: PendingApprovalEntryProps) => {
  const [isResponding, setIsResponding] = useState(false);
  const [hasResponded, setHasResponded] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const {
    isEnteringReason,
    denyReason,
    setIsEnteringReason,
    setDenyReason,
    clear,
  } = useApprovalForm(pendingStatus.approval_id);

  const { enableScope, disableScope, activeScopes } = useHotkeysContext();
  const tabNav = useContext(TabNavContext);
  const isLogsTabActive = tabNav ? tabNav.activeTab === 'logs' : true;
  const dialogScopeActive = activeScopes.includes(Scope.DIALOG);
  const shouldControlScopes = isLogsTabActive && !dialogScopeActive;
  const approvalsScopeEnabledRef = useRef(false);
  const dialogScopeActiveRef = useRef(dialogScopeActive);

  useEffect(() => {
    dialogScopeActiveRef.current = dialogScopeActive;
  }, [dialogScopeActive]);

  const { getPendingById } = useApprovals();
  const approvalInfo = getPendingById(pendingStatus.approval_id);

  const isQuestion = approvalInfo?.is_question ?? false;
  const questions = approvalInfo?.questions ?? null;

  const { timeLeft } = useApprovalCountdown(
    approvalInfo?.created_at ?? new Date().toISOString(),
    approvalInfo?.timeout_at ?? new Date().toISOString(),
    hasResponded
  );

  const disabled = isResponding || hasResponded || timeLeft <= 0;
  const shouldEnableApprovalsScope = shouldControlScopes && !disabled;

  useEffect(() => {
    const shouldEnable = shouldEnableApprovalsScope;
    if (shouldEnable && !approvalsScopeEnabledRef.current) {
      enableScope(Scope.APPROVALS);
      disableScope(Scope.KANBAN);
      approvalsScopeEnabledRef.current = true;
    } else if (!shouldEnable && approvalsScopeEnabledRef.current) {
      disableScope(Scope.APPROVALS);
      if (!dialogScopeActive) enableScope(Scope.KANBAN);
      approvalsScopeEnabledRef.current = false;
    }
    return () => {
      if (approvalsScopeEnabledRef.current) {
        disableScope(Scope.APPROVALS);
        if (!dialogScopeActiveRef.current) enableScope(Scope.KANBAN);
        approvalsScopeEnabledRef.current = false;
      }
    };
  }, [disableScope, enableScope, dialogScopeActive, shouldEnableApprovalsScope]);

  const respond = useCallback(
    async (approved: boolean, reason?: string) => {
      if (disabled) return;
      if (!executionProcessId) {
        setError('Missing executionProcessId');
        return;
      }
      setIsResponding(true);
      setError(null);
      const status: ApprovalStatus = approved
        ? { status: 'approved' }
        : { status: 'denied', reason };
      try {
        await approvalsApi.respond(pendingStatus.approval_id, {
          execution_process_id: executionProcessId,
          status,
        });
        setHasResponded(true);
        clear();
      } catch (e: unknown) {
        const errorMessage = e instanceof Error ? e.message : 'Failed to send response';
        setError(errorMessage);
      } finally {
        setIsResponding(false);
      }
    },
    [disabled, executionProcessId, pendingStatus.approval_id, clear]
  );

  const respondWithAnswers = useCallback(
    async (answers: QuestionAnswer[]) => {
      if (disabled) return;
      if (!executionProcessId) {
        setError('Missing executionProcessId');
        return;
      }
      setIsResponding(true);
      setError(null);
      try {
        await approvalsApi.respond(pendingStatus.approval_id, {
          execution_process_id: executionProcessId,
          status: { status: 'answered', answers },
        });
        setHasResponded(true);
      } catch (e: unknown) {
        const errorMessage = e instanceof Error ? e.message : 'Failed to submit answers';
        setError(errorMessage);
      } finally {
        setIsResponding(false);
      }
    },
    [disabled, executionProcessId, pendingStatus.approval_id]
  );

  const handleApprove = useCallback(() => respond(true), [respond]);
  const handleStartDeny = useCallback(() => {
    if (disabled) return;
    setError(null);
    setIsEnteringReason(true);
  }, [disabled, setIsEnteringReason]);
  const handleCancelDeny = useCallback(() => {
    if (isResponding) return;
    clear();
  }, [isResponding, clear]);
  const handleSubmitDeny = useCallback(() => {
    const trimmed = denyReason.trim();
    respond(false, trimmed || DEFAULT_DENIAL_REASON);
  }, [denyReason, respond]);
  const triggerDeny = useCallback(
    (event?: KeyboardEvent) => {
      if (!isEnteringReason || disabled || hasResponded) return;
      event?.preventDefault();
      handleSubmitDeny();
    },
    [isEnteringReason, disabled, hasResponded, handleSubmitDeny]
  );

  useKeyApproveRequest(handleApprove, {
    scope: Scope.APPROVALS,
    when: () => shouldEnableApprovalsScope && !isEnteringReason && !isQuestion,
    preventDefault: true,
  });
  useKeyDenyApproval(triggerDeny, {
    scope: Scope.APPROVALS,
    when: () => shouldEnableApprovalsScope && !hasResponded,
    enableOnFormTags: ['textarea', 'TEXTAREA'],
    preventDefault: true,
  });

  return (
    <div className="relative mt-3">
      <div className="overflow-hidden">
        {children}

        <div className="bg-background px-2 py-1.5 text-xs sm:text-sm">
          <TooltipProvider>
            {isQuestion && questions ? (
              // Question mode: render option selector
              <QuestionForm
                questions={questions}
                disabled={disabled}
                isResponding={isResponding}
                onSubmit={respondWithAnswers}
              />
            ) : (
              // Tool approval mode: render approve/deny buttons
              <div className="flex items-center justify-between gap-1.5 pl-4">
                <div className="flex items-center gap-1.5">
                  {!isEnteringReason && (
                    <span className="text-muted-foreground">
                      Would you like to approve this?
                    </span>
                  )}
                </div>
                {!isEnteringReason && (
                  <ActionButtons
                    disabled={disabled}
                    isResponding={isResponding}
                    onApprove={handleApprove}
                    onStartDeny={handleStartDeny}
                  />
                )}
              </div>
            )}

            {error && (
              <div className="mt-1 text-xs text-red-600" role="alert" aria-live="polite">
                {error}
              </div>
            )}

            {!isQuestion && isEnteringReason && !hasResponded && (
              <DenyReasonForm
                isResponding={isResponding}
                value={denyReason}
                onChange={setDenyReason}
                onCancel={handleCancelDeny}
                onSubmit={handleSubmitDeny}
              />
            )}
          </TooltipProvider>
        </div>
      </div>
    </div>
  );
};

export default PendingApprovalEntry;
```

- [ ] **Step 2: Run frontend type check**

```bash
pnpm run check 2>&1 | grep -E "error TS" | head -20
```

Fix any type errors. Common ones:
- `ApprovalOutcome` / `status` union doesn't include `answered` → check `shared/types.ts` has regenerated correctly
- `approvalsApi.respond` param type → should accept `ApprovalResponse` which uses `ApprovalOutcome`

- [ ] **Step 3: Run lint**

```bash
pnpm run lint 2>&1 | grep -E "error|warning" | head -20
```

Fix any lint issues.

- [ ] **Step 4: Commit**

```bash
git add packages/web-core/src/shared/components/NormalizedConversation/PendingApprovalEntry.tsx
git commit -m "feat: render question option-selector in PendingApprovalEntry for AskUserQuestion approvals"
```

---

## Task 10: End-to-end validation

- [ ] **Step 1: Build the full workspace**

```bash
cargo build --workspace 2>&1 | grep "error\[" | head -20
```

Expected: clean build.

- [ ] **Step 2: Run all Rust tests**

```bash
cargo test --workspace 2>&1 | tail -20
```

Expected: all pass.

- [ ] **Step 3: Run format**

```bash
pnpm run format
```

- [ ] **Step 4: Run lint**

```bash
pnpm run lint 2>&1 | grep "error" | head -20
```

Expected: no errors.

- [ ] **Step 5: Start dev server**

```bash
pnpm run dev
```

- [ ] **Step 6: Manually test plan approval flow**

Create a task in vibe-kanban configured with `plan: true`. Verify:
1. Agent runs in plan mode and presents a plan
2. `ExitPlanMode` triggers the approval UI (approve/deny buttons)
3. Approving transitions the agent to execution mode (bypass permissions)
4. Denying keeps or aborts the plan

- [ ] **Step 7: Manually test AskUserQuestion flow**

Create a task where Claude will call `AskUserQuestion` with options. Verify:
1. The approval entry shows the question text with radio/checkbox options
2. Selecting an option and submitting sends the correct answer
3. The agent receives the answer and continues

- [ ] **Step 8: Verify answer format**

With the dev server running, trigger an `AskUserQuestion`. After answering, inspect the backend log or add a temporary tracing log to confirm `PermissionResult::Allow { updated_input }` has `{ "answers": { "<header>": "<selected_label>" } }` not `{ "answers": { "<question_text>": "..." } }`.

- [ ] **Step 9: Final commit**

```bash
pnpm run format
git add -u
git commit -m "chore: format after end-to-end validation"
```

---

## Self-Review

**Spec coverage check:**
- ✅ Answer-key bug fixed (Task 5, `qa.header` instead of `qa.question`)
- ✅ `QuestionAnswer.header` added (Task 1)
- ✅ Structured questions flow to UI (Tasks 2–5)
- ✅ Question option-selection UI (Tasks 8–9)
- ✅ `permission_denials` captured and surfaced (Task 6)
- ✅ TypeScript types regenerated (Task 7)
- ✅ Plan approval (ExitPlanMode) unchanged — already works

**Placeholder scan:** No TBD or TODO in any code block above.

**Type consistency:**
- `AskUserQuestionItem` used in: `ExecutorApprovalService` trait, `create_internal`, `create_with_waiter`, `ApprovalInfo`, `QuestionForm` props — all consistent
- `QuestionAnswer.header` added and used in: `handle_question` answer map, `QuestionForm.onSubmit`, `respondWithAnswers` — consistent
- `ApprovalInfo.questions: Option<Vec<AskUserQuestionItem>>` matches `QuestionForm.questions: AskUserQuestionItem[]` — consistent
