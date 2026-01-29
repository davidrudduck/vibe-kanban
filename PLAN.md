# Plan: Fix Cross-Node Task and Attempt Viewing

## User Stories

### US-1: View Remote Task Details
**As a** swarm user on Node A,
**I want to** click on a task that was created on Node B,
**So that** I can see the task details without errors.

**Acceptance Criteria:**
- [ ] Task details panel loads without 404/500 errors
- [ ] Task title, description, status display correctly
- [ ] Task sub-routes (`/children`, `/variables`) return graceful empty responses

### US-2: View Remote Attempt Details
**As a** swarm user on Node A,
**I want to** view execution attempts that ran on Node B,
**So that** I can see the attempt branch, executor, and timestamps.

**Acceptance Criteria:**
- [ ] Attempt details load from Hive when not available locally
- [ ] Branch name, executor, timestamps display correctly
- [ ] Status indicators work (worktree deleted, setup completed)

### US-3: View Remote Attempt Status
**As a** swarm user viewing a remote attempt,
**I want to** see appropriate fallback values for local-only data,
**So that** the UI doesn't crash with 500 errors.

**Acceptance Criteria:**
- [ ] `/branch-status` returns graceful fallback (not 500)
- [ ] `/message-queue` returns empty array (not 500)
- [ ] `/children` returns empty relationships (not 500)
- [ ] `/has-session-error` returns false (not 500)

### US-4: No Console Errors on Remote Views
**As a** swarm user,
**I want** no JavaScript console errors when viewing remote tasks/attempts,
**So that** the application feels stable and professional.

**Acceptance Criteria:**
- [ ] No 404 errors in browser console for task sub-routes
- [ ] No 500 errors in browser console for attempt sub-routes
- [ ] WebSocket connections handled gracefully

## Problem Statement

Users viewing swarm projects from nodes that don't have the data locally see errors:
1. Task sub-routes (`/children`, `/variables`) return 404
2. Attempt sub-routes (`/branch-status`, `/message-queue`, `/children`) return 500
3. WebSocket connections fail for remote attempts

## Root Causes

### Root Cause 1: Remote tasks use `shared_task_id` as `task.id`
**File:** `crates/server/src/routes/tasks/handlers/core.rs:129-138`

When fetching tasks from Hive for merged view, the code sets:
```rust
id: shared_task.id,  // Should use a synthetic local ID or handle specially
shared_task_id: Some(shared_task.id),
```

This causes URLs like `/tasks/{shared_task_id}` which fail lookup.

### Root Cause 2: Middleware doesn't inject TaskAttempt for remote attempts
**File:** `crates/server/src/middleware/model_loaders.rs:438-447`

When attempt not found locally, middleware:
1. Inserts `RemoteAttemptNeeded` marker
2. Returns without inserting `TaskAttempt`
3. Handlers expecting `Extension<TaskAttempt>` fail with 500

### Root Cause 3: Task middleware whitelist too restrictive
**File:** `crates/server/src/middleware/model_loaders.rs:382-396`

Only `/tasks/{id}` and `/tasks/{id}/labels` support Hive fallback.
Missing: `/children`, `/variables/*`

## Solution Design

### Approach: Middleware-Level Hive Fetch

The cleanest fix is to have middleware fetch from Hive and inject the data when not found locally. This keeps handlers unchanged.

#### For Task Attempts:
1. When attempt not found locally, check if it's a GET request
2. If GET, try fetching from Hive via `RemoteClient::get_swarm_attempt()`
3. If found, convert `NodeTaskAttempt` to `TaskAttempt` and inject as extension
4. Also inject `RemoteTaskAttemptContext` for handlers that need to know it's remote

#### For Tasks:
1. Expand the whitelist to include more sub-routes that can safely fall back
2. For routes like `/children` and `/variables`, return empty data (no local data exists)

## Implementation Tasks

### Task 1: Update `load_task_attempt_impl` to fetch from Hive and inject TaskAttempt
**File:** `crates/server/src/middleware/model_loaders.rs`

**Current behavior (broken):**
```rust
Ok(None) => {
    // Attempt not found locally - signal handler to try Hive fallback
    request.extensions_mut().insert(RemoteAttemptNeeded { attempt_id });
    return Ok(next.run(request).await);  // No TaskAttempt injected!
}
```

**New behavior:**
```rust
Ok(None) => {
    // Attempt not found locally - fetch from Hive for GET requests
    if request.method() == Method::GET {
        let client = deployment.node_auth_client().cloned()
            .or_else(|| deployment.remote_client().ok());

        if let Some(client) = client {
            match client.get_swarm_attempt(task_attempt_id).await {
                Ok(response) => {
                    // Convert NodeTaskAttempt to TaskAttempt
                    let attempt = TaskAttempt {
                        id: response.attempt.id,
                        task_id: response.attempt.shared_task_id, // Use shared_task_id
                        container_ref: response.attempt.container_ref,
                        branch: response.attempt.branch,
                        target_branch: response.attempt.target_branch,
                        executor: response.attempt.executor,
                        worktree_deleted: response.attempt.worktree_deleted,
                        setup_completed_at: response.attempt.setup_completed_at,
                        created_at: response.attempt.created_at,
                        updated_at: response.attempt.updated_at,
                        hive_synced_at: Some(response.attempt.updated_at),
                        hive_assignment_id: response.attempt.assignment_id,
                        origin_node_id: Some(response.attempt.node_id),
                    };

                    // Inject RemoteTaskAttemptContext so handlers know it's remote
                    let remote_ctx = RemoteTaskAttemptContext {
                        node_id: response.attempt.node_id.parse().unwrap_or_default(),
                        node_url: None,  // Will need to look up from nodes
                        node_status: None,
                        task_id: response.attempt.shared_task_id,
                    };

                    request.extensions_mut().insert(attempt);
                    request.extensions_mut().insert(remote_ctx);
                    return Ok(next.run(request).await);
                }
                Err(e) if !e.is_not_found() => {
                    tracing::warn!(attempt_id = %task_attempt_id, error = %e,
                        "Failed to fetch attempt from Hive");
                }
                _ => {}
            }
        }
    }

    // For non-GET or Hive fetch failed - signal handler
    request.extensions_mut().insert(RemoteAttemptNeeded { attempt_id: task_attempt_id });
    return Ok(next.run(request).await);
}
```

**Key changes:**
1. Fetch from Hive inside middleware (not in handler)
2. Convert `NodeTaskAttempt` to `TaskAttempt` and inject it
3. Also inject `RemoteTaskAttemptContext` so handlers can detect it's remote
4. Fall back to `RemoteAttemptNeeded` only if Hive fetch fails

### Task 2: Update `load_task_middleware` to fetch from Hive and inject Task
**File:** `crates/server/src/middleware/model_loaders.rs`

Similar to Task 1, but for tasks. The current code only injects `RemoteTaskNeeded` for base route and `/labels`. We need to:

1. Fetch task from Hive when not found locally
2. Convert `SharedTask` to `Task` and inject it
3. Inject `RemoteTaskContext` marker (new type)
4. Allow more sub-routes to use Hive fallback

**New type needed:**
```rust
/// Marker extension indicating a task was fetched from Hive (not local).
#[derive(Debug, Clone)]
pub struct RemoteTaskContext {
    pub shared_task_id: Uuid,
    pub origin_node_id: Option<Uuid>,
}
```

### Task 3: Update task handlers to return fallback for remote tasks
**Files:** `crates/server/src/routes/tasks/handlers/*.rs`

After Task 2, handlers will have `Task` injected but can check for `RemoteTaskContext`:

`get_task_children`:
```rust
pub async fn get_task_children(
    Extension(task): Extension<Task>,
    remote_ctx: Option<Extension<RemoteTaskContext>>,  // Check if remote
    ...
) -> Result<...> {
    if remote_ctx.is_some() {
        // Return empty - children are local-only
        return Ok(ResponseJson(ApiResponse::success(vec![])));
    }
    // Existing local logic
}
```

`get_resolved_variables` (in task_variables.rs):
```rust
pub async fn get_resolved_variables(
    Extension(task): Extension<Task>,
    remote_ctx: Option<Extension<RemoteTaskContext>>,
    ...
) -> Result<...> {
    if remote_ctx.is_some() {
        // Return empty - variables are local-only
        return Ok(ResponseJson(ApiResponse::success(vec![])));
    }
    // Existing local logic
}
```

### Task 4: No changes needed for `get_tasks()`
The current behavior of using `shared_task_id` as `task.id` for remote tasks is actually correct because:
1. Task middleware now looks up by `shared_task_id` as fallback
2. Hive fetch uses `shared_task_id` to find the task
3. URLs will work: `/tasks/{shared_task_id}` → middleware finds it via Hive

### Task 5: Add tests for cross-node viewing scenarios
**File:** `crates/server/tests/cross_node_viewing.rs` (new)

Test cases:
1. View remote task from Hive fallback
2. View remote attempt from Hive fallback
3. View remote task children (empty result)
4. View remote task variables (empty result)
5. View remote attempt branch-status (fallback value)
6. View remote attempt message-queue (empty result)

## Validation Plan

### Manual Testing
1. From TheDoctor (no local SHRMS data):
   - Navigate to SHRMS project
   - Click on a task
   - Verify task details load
   - Verify attempt details load
   - Verify no console errors

2. From Tardis (has local SHRMS data):
   - Same navigation
   - Verify local data loads correctly
   - Verify no regressions

### Automated Testing
- Run `cargo test --workspace`
- Run `npm run check` in frontend

## Files to Modify

1. `crates/server/src/middleware/model_loaders.rs`
   - `load_task_attempt_impl()` - Add Hive fetch for GET requests
   - `load_task_middleware()` - Expand whitelist

2. `crates/server/src/routes/tasks/handlers/core.rs`
   - `get_task_children()` - Handle RemoteTaskNeeded

3. `crates/server/src/routes/task_variables.rs`
   - `get_resolved_variables()` - Handle RemoteTaskNeeded

4. `docs/architecture/swarm-api-patterns.mdx`
   - Update with new patterns if needed

## Risk Assessment

- **Low Risk**: Changes are additive (adding Hive fallback)
- **Medium Risk**: Middleware changes affect all routes
- **Mitigation**: Comprehensive testing on all three nodes

## Definition of Done

1. All SHRMS tasks viewable from TheDoctor
2. All attempts show details (not 500 errors)
3. No console errors when viewing remote tasks
4. `cargo test` passes
5. `npm run check` passes
6. Documentation updated
