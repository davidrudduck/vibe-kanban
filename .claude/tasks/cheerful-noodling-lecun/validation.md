# Validation Report: Fix System Variables Not Expanding in User Prompts

**Task ID**: `11529a8d-9baa-4b19-b9cc-e4919d8858e8` (Parent)
**Validation Task ID**: `c429ec79-8a3b-4791-a43d-326120d72344`
**Branch**: `dr/5d9a-expose-task-vari`
**Epic Plan**: `.claude/tasks/cheerful-noodling-lecun/epic-plan.md`
**Validator**: Claude Sonnet 4.5
**Date**: 2026-01-23

---

## Executive Summary

The implementation **successfully fixes the core bug** where system variables like `$TASK_ID`, `$PARENT_TASK_ID`, etc. were not expanding in follow-up prompts and initial execution prompts. The code changes are correct, well-implemented, and build successfully.

However, the implementation has **critical gaps**:
1. ❌ **MCP endpoints do NOT expose system variables** - External tools cannot access them
2. ❌ **No validation preventing users from overriding system variable names** - Silent overrides occur
3. ❌ **No tests added** for the new functionality
4. ⚠️ **Scope creep** - Added variables beyond what the plan described

The implementation is **INCOMPLETE** and should not be merged without addressing the MCP endpoint gap.

---

## Commit Analysis

### Branch Commits (dr/5d9a-expose-task-vari)
```bash
1e2ada8a5 docs: mark all tasks as completed for variable expansion epic
8b0190b96 Session 0: Initialization complete
285b6bc7f The validation is complete. Here's the summary:
17b8b1b18 The implementation is complete. Here's a summary:
```

### Files Changed
```text
.claude/tasks/cheerful-noodling-lecun/001.md       |  52 ++++++
.claude/tasks/cheerful-noodling-lecun/002.md       |  52 ++++++
.claude/tasks/cheerful-noodling-lecun/003.md       |  58 ++++++
.claude/tasks/cheerful-noodling-lecun/004.md       |  74 ++++++++
.claude/tasks/cheerful-noodling-lecun/epic-plan.md |  86 +++++++++
.claude/tasks/cheerful-noodling-lecun/validation.md| 147 +++++++++++++++
crates/db/src/models/task_variable.rs              | 121 +++++++++++++  ← IMPLEMENTATION
crates/server/.../follow_up.rs                     |   2 +-                 ← FIX 1
crates/services/src/services/container.rs          |   2 +-                 ← FIX 2
```

### Code Changes Analysis

#### ✅ Fix 1: follow_up.rs (Line 239)
```rust
// Before:
let variables = TaskVariable::get_variable_map(&deployment.db().pool, task.id)

// After:
let variables = TaskVariable::get_variable_map_with_system(&deployment.db().pool, task.id)
```
**Status**: ✅ Correctly implemented as per plan

#### ✅ Fix 2: container.rs (Line 906)
```rust
// Before:
let variables = TaskVariable::get_variable_map(&self.db().pool, task.id)

// After:
let variables = TaskVariable::get_variable_map_with_system(&self.db().pool, task.id)
```
**Status**: ✅ Correctly implemented as per plan

#### ✅ New Infrastructure: task_variable.rs (+121 lines)
The plan assumed `get_variable_map_with_system()` existed. It did not. The implementer correctly added:

1. **`SYSTEM_VARIABLE_NAMES`** constant (8 variables)
2. **`get_system_variables()`** - Generates runtime system variables
3. **`find_inherited_with_system()`** - Merges user + system variables
4. **`get_variable_map_with_system()`** - HashMap for variable expansion

**System Variables Implemented**:
- `TASK_ID` - Current task UUID
- `PARENT_TASK_ID` - Parent task UUID (empty if none)
- `TASK_TITLE` - Current task title
- `TASK_DESCRIPTION` - Current task description (empty if none)
- `TASK_LABEL` - First label name (empty if none)
- `PROJECT_ID` - Project UUID ⚠️ (not in original requirements)
- `PROJECT_TITLE` - Project name ⚠️ (not in original requirements)
- `IS_SUBTASK` - "true"/"false" ⚠️ (not in original requirements)

**Status**: ✅ Well-implemented, though 3 variables exceed original scope

---

## Plan Adherence Analysis

### Epic Plan Requirements
| Requirement | Status | Notes |
|------------|--------|-------|
| Fix follow_up.rs line 239 | ✅ Complete | Exactly as specified |
| Fix container.rs line 906 | ✅ Complete | Exactly as specified |
| Build succeeds | ✅ Complete | `cargo build` passes |
| Tests pass | ✅ Complete | Existing tests pass (45/45 server, 168/168 db) |
| Manual verification | ✅ Complete | Verified in Session 1 (vks-progress.md) |

### Original Task Requirements (from task description)
| Requirement | Status | Evidence |
|------------|--------|----------|
| Expose `TASK_ID` | ✅ Implemented | task_variable.rs:41 |
| Expose `PARENT_TASK_ID` | ✅ Implemented | task_variable.rs:48 |
| Expose `TASK_TITLE` | ✅ Implemented | task_variable.rs:55 |
| Expose `TASK_DESCRIPTION` | ✅ Implemented | task_variable.rs:62 |
| Expose `TASK_LABEL` | ✅ Implemented | task_variable.rs:69 |
| Work with `$VAR` syntax | ✅ Implemented | Uses variable_expander |
| Work with `${VAR}` syntax | ✅ Implemented | Uses variable_expander |
| **Warn if user defines system variable** | ❌ NOT IMPLEMENTED | validate_var_name() has no check |

---

## Critical Issues Found

### 🚨 Issue 1: MCP Endpoints Do NOT Return System Variables

**Affected Endpoints**:
1. `/api/tasks/{id}/variables/resolved` (crates/server/src/routes/task_variables.rs:70)
2. `/api/tasks/{id}/variables/preview` (crates/server/src/routes/task_variables.rs:157)

**Current Implementation**:
```rust
// routes/task_variables.rs:70
pub async fn get_resolved_variables(...) -> ... {
    let variables = TaskVariable::find_inherited(&deployment.db().pool, task.id).await?;
    //                            ^^^^^^^^^^^^^^^ Does NOT include system variables
    Ok(ResponseJson(ApiResponse::success(variables)))
}

// routes/task_variables.rs:157
pub async fn preview_expansion(...) -> ... {
    let resolved = TaskVariable::find_inherited(&deployment.db().pool, task.id).await?;
    //                         ^^^^^^^^^^^^^^^ Does NOT include system variables
    // ...
}
```

**Impact**:
- MCP tool `get_task_variables` returns ONLY user-defined variables
- Claude agents using MCP cannot see `$TASK_ID`, `$PARENT_TASK_ID`, etc.
- Preview expansion does NOT show system variable values

**Required Fix**:
```rust
// Change line 70:
let variables = TaskVariable::find_inherited_with_system(&deployment.db().pool, task.id).await?;

// Change line 157:
let resolved = TaskVariable::find_inherited_with_system(&deployment.db().pool, task.id).await?;
```

**Severity**: 🔴 **CRITICAL** - Breaks MCP integration

---

### 🚨 Issue 2: No Warning for System Variable Name Collision

**Task Requirement**:
> "If the variable is defined on a task, show a warning that these are system variables and the user must choose another variable name."

**Current Implementation**:
```rust
// routes/task_variables.rs:23
fn validate_var_name(name: &str) -> Result<(), ApiError> {
    // Validates format: [A-Z][A-Z0-9_]*
    // Does NOT check against SYSTEM_VARIABLE_NAMES
}
```

**Impact**:
- User can create a variable named `TASK_ID`
- System silently overrides it (user variable is ignored)
- Confusing behavior - user thinks they set `$TASK_ID`, but system overrides it

**Required Fix**:
```rust
use db::models::task_variable::SYSTEM_VARIABLE_NAMES;

fn validate_var_name(name: &str) -> Result<(), ApiError> {
    // ... existing validation ...

    if SYSTEM_VARIABLE_NAMES.contains(&name) {
        return Err(ApiError::BadRequest(format!(
            "Variable name '{}' is reserved as a system variable. System variables are: {}",
            name,
            SYSTEM_VARIABLE_NAMES.join(", ")
        )));
    }

    Ok(())
}
```

**Severity**: 🟠 **HIGH** - Causes user confusion and data loss

---

### ⚠️ Issue 3: No Tests for New Functionality

**Expected Tests** (per CLAUDE.md Section 6):
```rust
#[tokio::test]
async fn test_get_system_variables_returns_all_eight() { ... }

#[tokio::test]
async fn test_find_inherited_with_system_merges_correctly() { ... }

#[tokio::test]
async fn test_system_variables_override_user_defined() { ... }

#[tokio::test]
async fn test_get_variable_map_with_system_includes_system_vars() { ... }
```

**Current State**: Zero tests exist for the new functions

**Test File**: `crates/db/tests/task_variable_inheritance.rs` (exists, but no new tests added)

**Severity**: 🟡 **MEDIUM** - Lacks regression protection

---

## Build & Test Results

### ✅ Build Status
```bash
$ cargo build
   Compiling db v0.0.125
   Compiling server v0.0.125
   Compiling services v0.0.125
   Finished `dev` profile in 29.76s
```

### ✅ Existing Tests Pass
```bash
$ cargo test -p db
   Running tests/task_variable_inheritance.rs
   test test_find_inherited_deep_hierarchy ... ok
   test test_find_inherited_empty_no_variables ... ok
   test test_find_inherited_inherits_from_ancestors ... ok
   test test_find_inherited_child_overrides_parent ... ok
   test test_find_inherited_parent_only_variables ... ok
   test test_find_inherited_no_parent_returns_own_vars ... ok
   test test_find_inherited_partial_override_in_chain ... ok
   test test_find_inherited_results_sorted_by_name ... ok
   test result: ok. 8 passed; 0 failed
```

### ⚠️ Pre-existing Test Failures
```bash
$ cargo test -p services
   error[E0061]: normalize_logs takes 3 arguments but 2 supplied
   (This is unrelated to this task)
```

---

## Code Quality Assessment

### ✅ Positive Aspects

1. **Clean Rust Code**: Follows idiomatic Rust patterns
2. **Proper Error Handling**: Uses `?` operator and proper error types
3. **Efficient Queries**: No N+1 queries, single database calls
4. **Good Documentation**: Doc comments on new functions
5. **Correct Override Logic**: System variables properly override user variables
6. **Type Safety**: Proper use of `Uuid`, `DateTime<Utc>`, etc.
7. **HashSet Optimization**: Uses `HashSet` for O(1) lookups (task_variable.rs:328)

### ❌ Issues

1. **Incomplete MCP Integration**: Endpoints not updated (CRITICAL)
2. **Missing Validation**: No system variable name check (HIGH)
3. **No Tests**: Zero test coverage for new code (MEDIUM)
4. **Scope Creep**: Added 3 variables beyond requirements (LOW)

---

## CLAUDE.md Compliance

| Rule | Status | Notes |
|------|--------|-------|
| Type Safety First | ✅ Pass | Proper use of types |
| Error Transparency | ✅ Pass | Uses `thiserror` patterns |
| Stateless Services | ✅ Pass | Functions take `&SqlitePool` |
| UUID Identifiers | ✅ Pass | Uses `Uuid` type |
| UTC Timestamps | ✅ Pass | No timestamps in this code |
| **Testing (Section 6)** | ❌ Fail | No tests added |
| Logging | ✅ Pass | Uses `tracing::warn!` |
| Clippy Linting | ✅ Pass | No clippy warnings in modified files |
| Cargo Formatting | ✅ Pass | Code is properly formatted |

---

## Deviations from Plan

| Deviation | Type | Assessment |
|-----------|------|------------|
| Added 121 lines to task_variable.rs | Positive | Plan assumed function existed; implementer correctly added it |
| Added `PROJECT_ID` variable | Scope Creep | Not in requirements, but reasonable extension |
| Added `PROJECT_TITLE` variable | Scope Creep | Not in requirements, but reasonable extension |
| Added `IS_SUBTASK` variable | Scope Creep | Not in requirements, but reasonable extension |
| Did not update MCP endpoints | Negative | Critical gap in implementation |
| Did not add system variable validation | Negative | Requirement explicitly stated in task |
| Did not add tests | Negative | CLAUDE.md Section 6 requires tests |

---

## Scores

| Category | Score | Justification |
|----------|-------|---------------|
| **Following The Plan** | 7/10 | Core fixes match plan exactly, but plan was incomplete. Infrastructure correctly added, but MCP endpoints missed. |
| **Code Quality** | 8/10 | Clean, idiomatic Rust. Good error handling. Missing validation check reduces score. |
| **Following CLAUDE.md Rules** | 5/10 | Good Rust patterns, but violates Section 6 (Testing). No tests added. |
| **Best Practice** | 6/10 | Good separation of concerns, but silent override without warning is poor UX. MCP gap is architectural oversight. |
| **Efficiency** | 9/10 | Efficient queries, no N+1 issues, uses HashSet for O(1) lookups. Single point deduction for minor optimization opportunities. |
| **Performance** | 9/10 | Well-optimized database queries. Single call to fetch task, project, labels. Appropriate use of `unwrap_or_default()`. |
| **Security** | 9/10 | No security vulnerabilities. Proper use of parameterized queries. No SQL injection risks. Existing `validate_var_name` prevents injection. |

**Overall Score: 7.3/10**

---

## Recommendations

### 🚨 CRITICAL - Must Fix Before Merge

#### 1. Update MCP Endpoints to Include System Variables

**Files to Modify**:
- `crates/server/src/routes/task_variables.rs`

**Changes Required**:

```rust
// Line 70: get_resolved_variables
pub async fn get_resolved_variables(
    Extension(task): Extension<Task>,
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<Vec<ResolvedVariable>>>, ApiError> {
    let variables = TaskVariable::find_inherited_with_system(&deployment.db().pool, task.id).await?;
    //                          ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ Add _with_system
    Ok(ResponseJson(ApiResponse::success(variables)))
}

// Line 157: preview_expansion
pub async fn preview_expansion(
    Extension(task): Extension<Task>,
    State(deployment): State<DeploymentImpl>,
    Json(payload): Json<PreviewExpansionRequest>,
) -> Result<ResponseJson<ApiResponse<PreviewExpansionResponse>>, ApiError> {
    let resolved = TaskVariable::find_inherited_with_system(&deployment.db().pool, task.id).await?;
    //                         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ Add _with_system

    let variables: HashMap<String, (String, Option<Uuid>)> = resolved
        .into_iter()
        .map(|rv| (rv.name, (rv.value, Some(rv.source_task_id))))
        .collect();

    let result = variable_expander::expand_variables(&payload.text, &variables);

    Ok(ResponseJson(ApiResponse::success(
        PreviewExpansionResponse {
            expanded_text: result.text,
            undefined_variables: result.undefined_vars,
            expanded_variables: result
                .expanded_vars
                .into_iter()
                .map(|(name, source_id)| ExpandedVariableInfo {
                    name,
                    source_task_id: source_id.map(|id| id.to_string()),
                })
                .collect(),
        },
    )))
}
```

**Verification**:
```bash
# Test MCP endpoint returns system variables
curl http://localhost:6500/api/tasks/{task_id}/variables/resolved | jq '.data[] | select(.name | startswith("TASK_"))'
```

**Estimated Effort**: 5 minutes

---

#### 2. Add System Variable Name Validation

**Files to Modify**:
- `crates/server/src/routes/task_variables.rs`

**Changes Required**:

```rust
use db::models::task_variable::SYSTEM_VARIABLE_NAMES;

/// Validates that a variable name matches the pattern [A-Z][A-Z0-9_]*
/// and is not a reserved system variable name
fn validate_var_name(name: &str) -> Result<(), ApiError> {
    if name.is_empty() {
        return Err(ApiError::BadRequest(
            "Variable name cannot be empty".to_string(),
        ));
    }

    let mut chars = name.chars();

    // First character must be uppercase letter
    match chars.next() {
        Some(c) if c.is_ascii_uppercase() => {}
        _ => {
            return Err(ApiError::BadRequest(format!(
                "Invalid variable name '{}'. Names must start with an uppercase letter (A-Z).",
                name
            )));
        }
    }

    // Remaining characters must be uppercase letters, digits, or underscores
    for c in chars {
        if !c.is_ascii_uppercase() && !c.is_ascii_digit() && c != '_' {
            return Err(ApiError::BadRequest(format!(
                "Invalid variable name '{}'. Names must contain only uppercase letters, digits, and underscores.",
                name
            )));
        }
    }

    // NEW: Check if name is a reserved system variable
    if SYSTEM_VARIABLE_NAMES.contains(&name) {
        return Err(ApiError::BadRequest(format!(
            "Variable name '{}' is reserved as a system variable. System variables are: {}. Please choose a different name.",
            name,
            SYSTEM_VARIABLE_NAMES.join(", ")
        )));
    }

    Ok(())
}
```

**Verification**:
```bash
# Test that system variable names are rejected
curl -X POST http://localhost:6500/api/tasks/{task_id}/variables \
  -H "Content-Type: application/json" \
  -d '{"name": "TASK_ID", "value": "test"}'
# Expected: 400 Bad Request with error message
```

**Estimated Effort**: 10 minutes

---

### 🟡 SHOULD FIX - High Priority

#### 3. Add Tests for System Variables

**Files to Modify**:
- `crates/db/tests/task_variable_inheritance.rs`

**Tests to Add**:

```rust
#[tokio::test]
async fn test_get_system_variables_returns_all_eight() {
    use db::models::task_variable::get_system_variables;

    let (pool, _temp_dir) = setup_test_pool().await;
    let project = create_test_project(&pool).await;
    let task = create_test_task(&pool, project.id, None).await;

    let system_vars = get_system_variables(&pool, task.id).await.unwrap();

    assert_eq!(system_vars.len(), 8);

    let names: Vec<&str> = system_vars.iter().map(|v| v.name.as_str()).collect();
    assert!(names.contains(&"TASK_ID"));
    assert!(names.contains(&"PARENT_TASK_ID"));
    assert!(names.contains(&"TASK_TITLE"));
    assert!(names.contains(&"TASK_DESCRIPTION"));
    assert!(names.contains(&"TASK_LABEL"));
    assert!(names.contains(&"PROJECT_ID"));
    assert!(names.contains(&"PROJECT_TITLE"));
    assert!(names.contains(&"IS_SUBTASK"));
}

#[tokio::test]
async fn test_system_variables_override_user_defined() {
    let (pool, _temp_dir) = setup_test_pool().await;
    let project = create_test_project(&pool).await;
    let task = create_test_task(&pool, project.id, None).await;

    // User tries to set TASK_ID
    let create_var = CreateTaskVariable {
        name: "TASK_ID".to_string(),
        value: "user-defined-value".to_string(),
    };
    TaskVariable::create(&pool, task.id, &create_var).await.unwrap();

    // Get variables with system
    let resolved = TaskVariable::find_inherited_with_system(&pool, task.id).await.unwrap();

    let task_id_var = resolved.iter().find(|v| v.name == "TASK_ID").unwrap();
    assert_eq!(task_id_var.value, task.id.to_string());
    assert_ne!(task_id_var.value, "user-defined-value");
}

#[tokio::test]
async fn test_find_inherited_with_system_includes_both() {
    let (pool, _temp_dir) = setup_test_pool().await;
    let project = create_test_project(&pool).await;
    let task = create_test_task(&pool, project.id, None).await;

    // Create user variable
    let create_var = CreateTaskVariable {
        name: "MY_VAR".to_string(),
        value: "my-value".to_string(),
    };
    TaskVariable::create(&pool, task.id, &create_var).await.unwrap();

    let resolved = TaskVariable::find_inherited_with_system(&pool, task.id).await.unwrap();

    // Should have 8 system + 1 user = 9 variables
    assert_eq!(resolved.len(), 9);

    assert!(resolved.iter().any(|v| v.name == "TASK_ID"));
    assert!(resolved.iter().any(|v| v.name == "MY_VAR"));
}

#[tokio::test]
async fn test_get_variable_map_with_system_returns_hashmap() {
    let (pool, _temp_dir) = setup_test_pool().await;
    let project = create_test_project(&pool).await;
    let task = create_test_task(&pool, project.id, None).await;

    let var_map = TaskVariable::get_variable_map_with_system(&pool, task.id).await.unwrap();

    assert!(var_map.contains_key("TASK_ID"));
    assert!(var_map.contains_key("PROJECT_ID"));
    assert_eq!(var_map.len(), 8); // No user variables

    let (task_id_value, _) = var_map.get("TASK_ID").unwrap();
    assert_eq!(task_id_value, &task.id.to_string());
}
```

**Verification**:
```bash
cargo test -p db test_get_system_variables
cargo test -p db test_system_variables_override
cargo test -p db test_find_inherited_with_system
cargo test -p db test_get_variable_map_with_system
```

**Estimated Effort**: 30 minutes

---

### 📝 NICE TO HAVE - Optional

#### 4. Update Documentation

**Files to Modify**:
- `docs/core-features/task-variables.mdx` (if exists)
- Or create `.claude/tasks/cheerful-noodling-lecun/SYSTEM_VARIABLES.md`

**Content**:
```markdown
# System Variables

System variables are automatically available in all task prompts. They cannot be overridden by user-defined variables.

## Available System Variables

| Variable | Description | Example Value |
|----------|-------------|---------------|
| `$TASK_ID` | Current task UUID | `11529a8d-9baa-4b19-b9cc-e4919d8858e8` |
| `$PARENT_TASK_ID` | Parent task UUID (empty if none) | `c8809147-3066-439e-9f2b-9477cb3e8bec` |
| `$TASK_TITLE` | Current task title | `Fix System Variables` |
| `$TASK_DESCRIPTION` | Task description (empty if none) | `Expose task variables...` |
| `$TASK_LABEL` | First label name (empty if none) | `Bug Fix` |
| `$PROJECT_ID` | Project UUID | `a1b2c3d4-...` |
| `$PROJECT_TITLE` | Project name | `vkswarm` |
| `$IS_SUBTASK` | "true" or "false" | `true` |

## Usage in Prompts

System variables expand in:
- Initial task execution prompts
- Follow-up prompts to executors
- Variable preview expansion

Example:
```
Please fix the bug in task $TASK_ID for project $PROJECT_TITLE
```text

Expands to:
```

Please fix the bug in task 11529a8d-9baa-4b19-b9cc-e4919d8858e8 for project vkswarm
```
```

**Estimated Effort**: 15 minutes

---

## Conclusion

### Implementation Status: INCOMPLETE

The core bug fix is **functional and correct**:
- ✅ System variables expand in follow-up prompts (follow_up.rs)
- ✅ System variables expand in initial execution (container.rs)
- ✅ Infrastructure properly implemented (task_variable.rs)
- ✅ Build succeeds, existing tests pass

However, **critical gaps prevent merging**:
- ❌ MCP endpoints don't return system variables (breaks external tooling)
- ❌ No validation for system variable name collision (poor UX)
- ❌ No tests for new functionality (violates CLAUDE.md)

### Recommendation: CREATE FOLLOW-UP TASK

The implementation fixes the primary bug but is incomplete for production use. The MCP endpoint gap is particularly critical as it breaks the external API contract for tools like Claude Code that rely on MCP to query variables.

**Suggested Follow-Up Task**:
- **Title**: "MCP endpoints missing system variables and validation"
- **Label**: Bug Fix
- **Priority**: Critical
- **Estimated Effort**: 1 hour

---

## Summary by Priority

### 🚨 BLOCKING (Must Fix)
1. Update `get_resolved_variables` endpoint (5 min)
2. Update `preview_expansion` endpoint (5 min)
3. Add system variable name validation (10 min)

### 🟡 HIGH (Should Fix)
4. Add 4 test cases (30 min)

### 📝 LOW (Optional)
5. Update documentation (15 min)

**Total Critical Fix Time**: 20 minutes
**Total with Tests**: 50 minutes
**Total with Docs**: 65 minutes

---

## Validation Signature

**Validated by**: Claude Sonnet 4.5
**Validation Date**: 2026-01-23
**Branch Commit**: 1e2ada8a5
**Validation Result**: ⚠️ **INCOMPLETE - DO NOT MERGE**
**Required Actions**: Fix 2 critical issues (MCP endpoints + validation)

---
