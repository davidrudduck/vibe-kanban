# Task Name: Fix System Variables Not Expanding in User Prompts

## Overview
System variables (`$TASK_ID`, `$PARENT_TASK_ID`, `$PROJECT_ID`, etc.) are not being expanded when users send follow-up prompts to executors. User-defined variables like `$TASKPLAN` expand correctly.

**Root Cause**: Two code locations use `TaskVariable::get_variable_map()` instead of `TaskVariable::get_variable_map_with_system()`, which excludes system variables from expansion.

**Evidence**: Screenshot shows literal `$TASK_ID`, `$PARENT_TASK_ID`, `$PROJECT_ID` in executor prompt while `TASKPLAN` (user-defined) was expanded correctly.

---

## Implementation Sessions

### Session 1 - **Fix Variable Expansion in Follow-Up and Initial Execution** [DONE]

#### User Stories
US1: As a user, I want to use $TASK_ID in my follow-up prompts so I can reference the current task ID.
US2: As a user, I want system variables like $PROJECT_TITLE to expand in my prompts without declaring them.

#### Files to Modify

| File | Change |
|------|--------|
| `crates/server/src/routes/task_attempts/handlers/follow_up.rs` | Line 239: Change `get_variable_map` to `get_variable_map_with_system` |
| `crates/services/src/services/container.rs` | Line 906: Change `get_variable_map` to `get_variable_map_with_system` |

#### Step 1. Fix follow_up.rs [DONE]
**Location**: `crates/server/src/routes/task_attempts/handlers/follow_up.rs` line 239

**Change from**:
```rust
let variables = TaskVariable::get_variable_map(&deployment.db().pool, task.id)
```

**Change to**:
```rust
let variables = TaskVariable::get_variable_map_with_system(&deployment.db().pool, task.id)
```

#### Step 2. Fix container.rs [DONE]
**Location**: `crates/services/src/services/container.rs` line 906

**Change from**:
```rust
let variables = TaskVariable::get_variable_map(&self.db().pool, task.id)
```

**Change to**:
```rust
let variables = TaskVariable::get_variable_map_with_system(&self.db().pool, task.id)
```

#### Tests
1. Verify `cargo build` succeeds
2. Verify `cargo test -p server -p services` passes
3. Manual test: Send follow-up prompt with `$TASK_ID` and verify it expands to actual UUID

---

## Verification

1. Build the project: `cargo build`
2. Run tests: `cargo test -p server -p services`
3. Start the server and send a follow-up message containing `$TASK_ID` - verify it expands to the actual task UUID
4. Verify `$PROJECT_TITLE`, `$PARENT_TASK_ID`, `$IS_SUBTASK` also expand correctly

---

## Success Criteria
- All 8 system variables (TASK_ID, PARENT_TASK_ID, TASK_TITLE, TASK_DESCRIPTION, TASK_LABEL, PROJECT_ID, PROJECT_TITLE, IS_SUBTASK) expand correctly in:
  - Follow-up prompts
  - Initial execution prompts
- Existing tests continue to pass

---

## Tasks Created
- [x] 001.md - Fix variable expansion in follow_up.rs (parallel: true) - COMPLETED
- [x] 002.md - Fix variable expansion in container.rs (parallel: true) - COMPLETED
- [x] 003.md - Build and test the changes (parallel: false) - COMPLETED
- [x] 004.md - Manual verification of system variable expansion (parallel: false) - COMPLETED

Total tasks: 4
Parallel tasks: 2 (001, 002)
Sequential tasks: 2 (003, 004)
Estimated total effort: 1.25 hours
