# Plan Design
Use the following format as a template for how the plan must be broken down. The plan must provide clear details of what is to be implemented as part of this task and why, ensuring consistency of implementation across multiple implementation sessions that will not share context.

## Development Principles
- Follow Extreme Programming Test Driven Design (TDD) principles
- Keep It Simple Stupid (KISS) - don't over complicate the implementation
- You Aren't Going To Need It (YAGNI) - don't implement features that are not requested

## User Stories
The plan must include 3-5 user stories that outline the behaviour the user expects as a result of the implementation of this plan.

## Test Driven Design
Follow Test Driven Design (TDD), write tests FIRST.
- Identify The Need: Determine the next small piece of functionality or behavior required.
- Write A Test (Red): Create a test for that behavior (e.g., test_addition_of_two_numbers).
- Make it Pass (Green): Add just enough code (e.g., return a + b;) so the test runs and passes.
- Clean Up (Refactor): Simplify the code and tests for clarity, ensuring tests remain green.
- Repeat: Pick the next test from your list and repeat the cycle, growing the software incrementally. 

### The Red-Green-Refactor Cycle
- *Red*: Write a small, specific unit test for a new piece of functionality that you know will fail because the code doesn't exist yet.
- *Green*: Write the minimum amount of production code required to make that failing test pass.
- *Refactor*: Improve the design, structure, and readability of both the new and existing code without changing its behavior, ensuring all tests still pass. 

## Sessions
Using the 'Test Driven Design' above, break the plan into sessions following TDD principles.

- Sessions are incremental whole number integers
- The first session must be 1 (one) - it cannot be 0 (zero)
- Each session should be able to be completed in 20 assistant messages or less
- There must be a session dedicated to creating or updating documentation in the same format as already used
   - This includes user documentation in docs/ and architecture documentation in docs/architecture/

### Session Details
Each session MUST include:
- 1-3 user stories 
- 3-5 tests that will validate the successful implementation of this session
- What file(s) do you need to create, edit, update or delete?
- Code sample for changes
- 3-5 steps to be performed

### Session Guidelines
The session must list all components relevant to that session like database schema, api endpoint, ui/ux design elements.

```markdown
# Task Name:  Example Task

## Overview
What is the core purpose of this task? What are the goals and objectives? Why are we doing this? What problem does it solve?

## Implementation Sessions

### Session 1 - **Initialization**		[    ]

#### Feature - Node-Side Backfill Request Handler
- Handles `HiveEvent::BackfillRequest` in `node_runner.rs` event loop
- Queries local database for requested attempt data
- Sends `AttemptSync`, `ExecutionSync`, `LogsBatch` messages to Hive
- Sends `BackfillResponseMessage` when complete

#### User Stories
US1: Cross-Node Attempt Viewing
As a user viewing a task attempt from another node, I want the Hive to automatically fetch missing data so that I see complete execution details.

#### Components
##### Database Schema
###### Table 1
- column 1, column 2, column 3
- larger column (JSON)

##### API Endpoints
###### Example Endpoint 1
- POST /api/example/list
- GET /api/example/get

#### Files to Modify

| File | Changes |
|------|---------|
| `crates/services/src/services/node_runner.rs` | Add backfill handler in event loop (~50 lines) |
| `crates/remote/src/state.rs` | Add `backfill: Arc<BackfillService>` field |

#### Step 1. Node-Side Backfill Handler	[    ]
**Location**: `crates/services/src/services/node_runner.rs` lines 658-809 (main event loop)
**Pattern to follow**: Similar to `HiveEvent::LabelSync` handling (lines 758-797)
```rust
Some(HiveEvent::BackfillRequest(request)) => {
    let message_id = request.message_id;
    let mut entities_sent = 0u32;
    let mut error: Option<String> = None;
    for attempt_id in &request.entity_ids {
        match request.backfill_type {
            BackfillType::FullAttempt => {
                // 1. Query TaskAttempt::find_by_id()
                // 2. Send AttemptSyncMessage via command_tx
                // 3. Query ExecutionProcess::find_by_task_attempt_id()
                // 4. For each execution, send ExecutionSyncMessage
                // 5. Query DbLogEntry::find_by_execution_id()
                // 6. Send LogsBatchMessage
                entities_sent += 1;
            }
            BackfillType::Executions => { /* executions only */ }
            BackfillType::Logs => { /* logs only, use logs_after filter */ }
        }
    }
    // Send response
    let response = BackfillResponseMessage {
        request_id: message_id,
        success: error.is_none(),
        error,
        entities_sent,
    };
    let _ = command_tx.send(NodeMessage::BackfillResponse(response)).await;
}
```
**Data access**: Use `db.pool` for queries, `command_tx` for sending messages

#### Step 2. On-Demand Trigger in get_node_task_attempt		[    ]
**File**: `crates/remote/src/routes/nodes.rs` (after line 1151)
```rust
// After org access verified, before querying executions
if attempt.sync_state == "partial" {
    let backfill = state.backfill().clone();
    let node_id = attempt.node_id;
    let attempt_id = attempt.id;
    // Non-blocking backfill trigger
    tokio::spawn(async move {
        if let Err(e) = backfill.request_immediate_backfill(node_id, attempt_id).await {
            tracing::debug!(
                node_id = %node_id,
                attempt_id = %attempt_id,
                error = %e,
                "on-demand backfill request failed (node may be offline)"
            );
        }
    });
}
```

#### Tests
1. `test_backfill_full_attempt` - Verify FullAttempt sends AttemptSync, ExecutionSync, LogsBatch
2. `test_backfill_executions_only` - Verify Executions type sends only ExecutionSync

---

### Session 2 - **Build Core Interface**		[    ]
#### Feature - Node-Side Backfill Request Handler
- Handles `HiveEvent::BackfillRequest` in `node_runner.rs` event loop
- Queries local database for requested attempt data
- Sends `AttemptSync`, `ExecutionSync`, `LogsBatch` messages to Hive
- Sends `BackfillResponseMessage` when complete

#### User Stories
US2: Cross-Node Attempt Viewing
As a user viewing a task attempt from another node, I want the Hive to automatically fetch missing data so that I see complete execution details.

#### Components
##### UI Layout
###### Overall Layout
- Responsive breakpoints: mobile (single column), tablet (two column), desktop (three column)
- Persistent header with project selector

###### Side Menu Left
- Settings
- Login/Logout

###### Main Focus Area
- View main data

##### Design
###### Color Palette
- Primary: Orange/amber accent (#CC785C claude-style)
- Background: White (light mode), Dark gray (#1A1A1A dark mode)
- Surface: Light gray (#F5F5F5 light), Darker gray (#2A2A2A dark)
- Text: Near black (#1A1A1A light), Off-white (#E5E5E5 dark)
- Borders: Light gray (#E5E5E5 light), Dark gray (#404040 dark)
- Code blocks: Monaco editor theme

###### Typography
- Sans-serif system font stack (Inter, SF Pro, Roboto, system-ui)
- Headings: font-semibold
- Body: font-normal, leading-relaxed
- Code: Monospace (JetBrains Mono, Consolas, Monaco)
- Message text: text-base (16px), comfortable line-height

##### Interface Components
###### Conversation Log
- User messages: Right-aligned, subtle background
- Executor messages: Left-aligned, no background
- Markdown formatting with proper spacing
- Inline code with bg-gray-100 background
- Code blocks with syntax highlighting
- Copy button on code blocks

###### Buttons
- Primary: Orange/amber background, white text, rounded
- Secondary: Border style with hover fill
- Icon buttons: Square with hover background
- Disabled state: Reduced opacity, no pointer events

###### Inputs
- Rounded borders with focus ring
- Textarea auto-resize
- Placeholder text in gray
- Error states in red
- Character counter

###### Cards
- Subtle border or shadow
- Rounded corners (8px)
- Padding: p-4 to p-6
- Hover state: slight shadow increase

###### Animations
- Smooth transitions (150-300ms)
- Fade in for new messages
- Slide in for sidebar
- Typing indicator animation
- Loading spinner for generation
- Skeleton loaders for content

#### Files to Modify
| File | Changes |
|------|---------|
| `crates/services/src/services/node_runner.rs` | Add backfill handler in event loop (~50 lines) |
| `crates/remote/src/state.rs` | Add `backfill: Arc<BackfillService>` field |

#### Step 1. Reconnect Trigger in session.rs			[    ]
**File**: `crates/remote/src/nodes/ws/mod.rs` - extract and pass backfill
**File**: `crates/remote/src/nodes/ws/session.rs` - add after line 117
```rust
pub async fn handle(
    socket: WebSocket,
    pool: PgPool,
    connections: ConnectionManager,
    backfill: Arc<BackfillService>,  // NEW PARAM
) {
    // ... existing auth code ...
    // After line 117: connections.register(...)
    // Trigger reconnect backfill (non-blocking)
    tokio::spawn({
        let backfill = backfill.clone();
        let node_id = auth_result.node_id;
        async move {
            match backfill.trigger_reconnect_backfill(node_id).await {
                Ok(count) if count > 0 => {
                    tracing::info!(node_id = %node_id, count, "triggered reconnect backfill");
                }
                Err(e) => {
                    tracing::warn!(node_id = %node_id, error = %e, "reconnect backfill failed");
                }
                _ => {}
            }
        }
    });
    // Continue with broadcast_node_projects...
}
```

#### Step 2. On-Demand Trigger in get_node_task_attempt		[     ]
**File**: `crates/remote/src/routes/nodes.rs` (after line 1151)
```rust
// After org access verified, before querying executions
if attempt.sync_state == "partial" {
    let backfill = state.backfill().clone();
    let node_id = attempt.node_id;
    let attempt_id = attempt.id;
    // Non-blocking backfill trigger
    tokio::spawn(async move {
        if let Err(e) = backfill.request_immediate_backfill(node_id, attempt_id).await {
            tracing::debug!(
                node_id = %node_id,
                attempt_id = %attempt_id,
                error = %e,
                "on-demand backfill request failed (node may be offline)"
            );
        }
    });
}
```

#### Tests
1. `test_backfill_full_attempt` - Verify FullAttempt sends AttemptSync, ExecutionSync, LogsBatch
2. `test_backfill_executions_only` - Verify Executions type sends only ExecutionSync

---

## Success Criteria

### Functionality
- Full CRUD for Projects
- Full CRUD for Tasks
- Multi-node execution of Task Attempts

### User Experience
- Responsive on all devices
- Fast response times and minimal lag
- Intuitive navigation and workflows
- All items render correctly
- No light text on light background or dark text on dark background

### Technical Quality
- Clean, maintainable code structure
- Proper error handling throughout
- Secure API key management
- Optimized database queries
- Comprehensive testing coverage

### Design Polish
- Consistent visual design
- Beautiful typography and spacing
- Smooth animations and micro-interactions
- Excellent contrast and accessibility
- Professional, polished appearance
- Dark mode fully implemented

```
  
# Plan Approval
When the plan is approved:
1. Write the plan, but DO NOT BEGIN IMPLEMENTATION OF THE PLAN.
2. Create new task for implementation of the plan:
  - Name the task "[outcome]" where [outcome] is a brief summary of the goals
  - Do not start the task
  - Enter the description as "@start"
3. Set the variable `TASKPLAN` on this newly created task to the full path and filename of the plan you created.
4. Set the variable `SESSIONSMAX` on this newly created task to the total number of sessions in the plan you created.

** Use `mcp__vkswarm__set_variable` **

