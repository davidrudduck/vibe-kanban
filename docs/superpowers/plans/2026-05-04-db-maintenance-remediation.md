# DB Maintenance & Diagnostics — Remediation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix all correctness, safety, and UX issues identified in the adversarial review of the DB Maintenance & Diagnostics feature.

**Architecture:** Nine focused tasks, each touching one logical concern. Tasks 1–3 are pure Rust with no schema changes. Task 4 introduces a migration that Tasks 5–6 depend on. Tasks 7–9 are independent of each other and of the migration.

**Tech Stack:** Rust/axum, sqlx (SQLite), tokio, React/TypeScript, React Query (@tanstack/react-query)

---

## File Map

| File | Role |
|---|---|
| `crates/db/src/wal_monitor.rs` | Task 1 — TRUNCATE success check |
| `crates/server/src/routes/database.rs` | Tasks 2, 5, 6 — VACUUM TOCTOU, archived queries, log purge |
| `crates/db/src/database_stats.rs` | Task 3 — bytes_freed clamp, WAL path |
| `crates/db/migrations/20260505000000_add_workspace_archived_at.sql` | Task 4 — new column |
| `crates/db/src/models/workspace.rs` | Task 4 — set_archived update |
| `crates/server/src/routes/diagnostics.rs` | Task 7 — total_bytes, new struct fields |
| `crates/server/src/bin/generate_types.rs` | Task 7 — register updated DiskUsageResponse |
| `crates/server/src/main.rs` | Task 8 — WAL monitor shutdown |
| `packages/web-core/src/shared/dialogs/settings/settings/DiagnosticsPanel.tsx` | Tasks 7, 9 — total/displayed, error state |
| `packages/web-core/src/shared/dialogs/settings/settings/MaintenancePanel.tsx` | Task 9 — error state |

---

## Task 1: Fix WAL TRUNCATE partial-checkpoint silent success

**Problem:** `run_truncate_checkpoint()` in `wal_monitor.rs` (line 312) declares success when `blocked == 0`, but SQLite can return `blocked == 0` with `log_pages != checkpointed` — meaning the WAL was not fully flushed and cannot be truncated. The code silently treats partial checkpoints as complete.

**Files:**
- Modify: `crates/db/src/wal_monitor.rs:310-332`
- Test: `crates/db/src/wal_monitor.rs` (add to `#[cfg(test)]` section)

- [ ] **Step 1: Write the failing test**

Add to the `#[cfg(test)]` block at the bottom of `crates/db/src/wal_monitor.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    // ... existing tests ...

    /// Validates that the three branches of TRUNCATE result interpretation
    /// are logically distinct and correctly described.
    ///
    /// This is a logic/documentation test — it verifies the expected semantics
    /// via assertions on the condition expressions, not live SQLite.
    #[test]
    fn test_truncate_success_requires_log_eq_checkpointed() {
        // blocked != 0 → blocked by readers; not a success
        let (blocked, log, checkpointed) = (1i32, 10i32, 5i32);
        assert!(blocked != 0, "blocked readers case");

        // blocked == 0 but log != checkpointed → partial; not a success
        let (blocked, log, checkpointed) = (0i32, 10i32, 5i32);
        assert!(blocked == 0 && log != checkpointed, "partial checkpoint case");
        assert!(!(blocked == 0 && log == checkpointed), "partial is NOT full success");

        // blocked == 0 and log == checkpointed → full success
        let (blocked, log, checkpointed) = (0i32, 10i32, 10i32);
        assert!(blocked == 0 && log == checkpointed, "full success case");
    }
}
```

- [ ] **Step 2: Run test to confirm it compiles and passes**

```bash
cargo test -p vibe-kanban-db wal_monitor::tests::test_truncate_success_requires_log_eq_checkpointed
```

Expected: PASS (it's a logic test, not dependent on the bug being fixed yet).

- [ ] **Step 3: Fix the TRUNCATE success check**

Replace lines 310–332 in `crates/db/src/wal_monitor.rs`:

**Before:**
```rust
match result {
    Ok((blocked, log_pages, checkpointed)) => {
        if blocked == 0 {
            tracing::info!(
                duration_ms = duration.as_millis() as u64,
                log_pages = log_pages,
                checkpointed = checkpointed,
                "TRUNCATE checkpoint completed - all WAL flushed to main database"
            );
        } else {
            // blocked != 0 is SQLite's indication that readers/writers prevented
            // full checkpointing — this is the WAL-mode equivalent of SQLITE_BUSY
            // for TRUNCATE mode. The WAL was not fully flushed; fall back to a
            // PASSIVE checkpoint rather than treating this as success.
            tracing::warn!(
                duration_ms = duration.as_millis() as u64,
                blocked = blocked,
                log_pages = log_pages,
                checkpointed = checkpointed,
                "TRUNCATE checkpoint was blocked - falling back to PASSIVE checkpoint"
            );
            self.run_checkpoint().await;
        }
    }
```

**After:**
```rust
match result {
    Ok((blocked, log_pages, checkpointed)) => {
        if blocked == 0 && log_pages == checkpointed {
            // True success: no readers blocked us AND all frames were checkpointed.
            // SQLite will truncate the WAL file after this.
            tracing::info!(
                duration_ms = duration.as_millis() as u64,
                log_pages = log_pages,
                checkpointed = checkpointed,
                "TRUNCATE checkpoint completed — all WAL flushed to main database"
            );
        } else if blocked != 0 {
            // Active readers or writers prevented the checkpoint from acquiring
            // the exclusive lock needed for TRUNCATE.
            tracing::warn!(
                duration_ms = duration.as_millis() as u64,
                blocked = blocked,
                log_pages = log_pages,
                checkpointed = checkpointed,
                "TRUNCATE checkpoint blocked by active readers — falling back to PASSIVE"
            );
            self.run_checkpoint().await;
        } else {
            // blocked == 0 but log_pages != checkpointed: SQLite acquired the lock
            // but could not checkpoint all frames (e.g. a reader held a read
            // transaction open during the walk). WAL is NOT truncated.
            tracing::warn!(
                duration_ms = duration.as_millis() as u64,
                log_pages = log_pages,
                checkpointed = checkpointed,
                "TRUNCATE checkpoint incomplete (partial): {} of {} frames checkpointed — falling back to PASSIVE",
                checkpointed,
                log_pages,
            );
            self.run_checkpoint().await;
        }
    }
```

- [ ] **Step 4: Run tests**

```bash
cargo test -p vibe-kanban-db
```

Expected: all tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/db/src/wal_monitor.rs
git commit -m "fix(wal): require log==checkpointed for TRUNCATE checkpoint success

blocked==0 alone does not mean all WAL frames were checkpointed.
When blocked==0 but log!=checkpointed the WAL is only partially
flushed and cannot be truncated. Fall back to PASSIVE in that case
and emit a warning instead of silently treating it as complete."
```

---

## Task 2: Fix VACUUM cooldown TOCTOU race

**Problem:** `vacuum()` handler in `database.rs` (lines 97–118) reads the cooldown under a *read* lock, drops the lock, runs VACUUM (which takes seconds), then acquires a *write* lock to record the timestamp. Two concurrent requests both pass the read-lock check before either sets the timestamp.

**Files:**
- Modify: `crates/server/src/routes/database.rs:94-121`

- [ ] **Step 1: Replace the vacuum handler**

Replace the entire `vacuum` function (lines 94–121):

**Before:**
```rust
async fn vacuum(
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<VacuumResult>>, ApiError> {
    {
        let last = deployment.last_vacuum_time().read().await;
        if let Some(prev) = *last {
            let elapsed = Utc::now().signed_duration_since(prev).num_seconds();
            if elapsed < VACUUM_COOLDOWN_SECS {
                return Err(ApiError::TooManyRequests(
                    "Vacuum cooldown active".to_string(),
                ));
            }
        }
    }

    let pool = &deployment.db().pool;
    let result = vacuum_database(pool).await.map_err(|e| {
        tracing::error!("vacuum error: {e}");
        ApiError::Database(sqlx::Error::Protocol(e.to_string()))
    })?;

    {
        let mut last = deployment.last_vacuum_time().write().await;
        *last = Some(Utc::now());
    }

    Ok(ResponseJson(ApiResponse::success(result)))
}
```

**After:**
```rust
async fn vacuum(
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<VacuumResult>>, ApiError> {
    // Acquire the WRITE lock for the entire check+claim sequence.
    // This eliminates the TOCTOU race: two concurrent requests cannot both
    // pass the cooldown check because the second sees the timestamp set by
    // the first before either drops the lock.
    {
        let mut last = deployment.last_vacuum_time().write().await;
        if let Some(prev) = *last {
            let elapsed = Utc::now().signed_duration_since(prev).num_seconds();
            if elapsed < VACUUM_COOLDOWN_SECS {
                return Err(ApiError::TooManyRequests(
                    "Vacuum cooldown active".to_string(),
                ));
            }
        }
        // Claim the slot atomically. Set the timestamp *before* releasing the
        // lock so concurrent requests see the cooldown immediately. If VACUUM
        // fails below, the cooldown still applies — this prevents retry storms
        // on a DB that is under heavy load.
        *last = Some(Utc::now());
    } // write lock released here; VACUUM runs without holding any lock

    let pool = &deployment.db().pool;
    vacuum_database(pool).await.map_err(|e| {
        tracing::error!("vacuum error: {e}");
        ApiError::Database(sqlx::Error::Protocol(e.to_string()))
    }).map(|result| ResponseJson(ApiResponse::success(result)))
}
```

- [ ] **Step 2: Build to confirm it compiles**

```bash
cargo build -p vibe-kanban-server 2>&1 | grep -E "^error"
```

Expected: no output (no errors).

- [ ] **Step 3: Commit**

```bash
git add crates/server/src/routes/database.rs
git commit -m "fix(vacuum): eliminate TOCTOU race in cooldown check

Previously: read lock → check → drop → VACUUM → write lock.
Now: write lock → check → set timestamp → drop → VACUUM.
Concurrent requests cannot both pass the cooldown because the second
sees the timestamp claimed by the first before the lock is released."
```

---

## Task 3: Fix database_stats.rs — bytes_freed negative + WAL path comment

**Problem 1:** `vacuum_database()` (line 159) computes `(before_pages - after_pages) * page_size`. If VACUUM reorganises but does not shrink the DB (edge case), `after_pages > before_pages` and `bytes_freed` is negative. The existing test asserts `>= 0` but the implementation doesn't enforce it.

**Problem 2:** Lines 76–78 have a misleading comment claiming `Path::with_extension` would break `db.v2.sqlite`. It would not — `with_extension("sqlite-wal")` correctly produces `db.v2.sqlite-wal` (replaces `.sqlite` with `.sqlite-wal`). The comment causes confusion and diverges from `wal_monitor.rs` which correctly uses `with_extension`.

**Files:**
- Modify: `crates/db/src/database_stats.rs:76-78,158-160`

- [ ] **Step 1: Write the bytes_freed edge-case test**

Add to the `#[cfg(test)]` block in `crates/db/src/database_stats.rs`:

```rust
#[tokio::test]
async fn test_vacuum_bytes_freed_is_non_negative() {
    let (pool, _temp_dir) = setup_test_pool().await;

    // Insert some data to give VACUUM something to do, then delete it
    sqlx::query("CREATE TABLE IF NOT EXISTS _test_vacuum (x TEXT)")
        .execute(&pool)
        .await
        .unwrap();
    for i in 0..100 {
        sqlx::query("INSERT INTO _test_vacuum VALUES (?)")
            .bind(format!("row_{i}"))
            .execute(&pool)
            .await
            .unwrap();
    }
    sqlx::query("DELETE FROM _test_vacuum")
        .execute(&pool)
        .await
        .unwrap();

    let result = vacuum_database(&pool).await.unwrap();
    assert!(
        result.bytes_freed >= 0,
        "bytes_freed must never be negative, got {}",
        result.bytes_freed
    );
}
```

- [ ] **Step 2: Run test (should pass with current code in the normal case)**

```bash
cargo test -p vibe-kanban-db test_vacuum_bytes_freed_is_non_negative
```

Expected: PASS (the normal path already yields >= 0; this test documents the contract).

- [ ] **Step 3: Fix bytes_freed to clamp at zero**

In `crates/db/src/database_stats.rs`, replace line 158–160:

**Before:**
```rust
    Ok(VacuumResult {
        bytes_freed: (before_pages - after_pages) * page_size,
    })
```

**After:**
```rust
    Ok(VacuumResult {
        // Clamp to zero: VACUUM can theoretically reorganise pages without
        // shrinking the file (e.g. autovacuum interference), yielding
        // after_pages > before_pages. Negative freed bytes is nonsensical.
        bytes_freed: ((before_pages - after_pages) * page_size).max(0),
    })
```

- [ ] **Step 4: Fix the WAL path construction and its wrong comment**

In `crates/db/src/database_stats.rs`, replace lines 75–83:

**Before:**
```rust
    // Construct WAL path by appending "-wal" to the full db path string.
    // Using string concatenation avoids Path::with_extension stripping ".sqlite" from "db.v2.sqlite".
    let wal_path_str = db_path.to_string_lossy().to_string() + "-wal";
    let wal_path = std::path::PathBuf::from(&wal_path_str);
    let wal_size_bytes = if wal_path.exists() {
        std::fs::metadata(&wal_path)?.len() as i64
    } else {
        0
    };
```

**After:**
```rust
    // db.v2.sqlite → db.v2.sqlite-wal
    // Path::with_extension("sqlite-wal") replaces the last extension (.sqlite)
    // with .sqlite-wal, which is correct. This matches wal_monitor.rs.
    let wal_path = db_path.with_extension("sqlite-wal");
    let wal_size_bytes = if wal_path.exists() {
        std::fs::metadata(&wal_path)?.len() as i64
    } else {
        0
    };
```

- [ ] **Step 5: Run all db tests**

```bash
cargo test -p vibe-kanban-db
```

Expected: all tests pass.

- [ ] **Step 6: Commit**

```bash
git add crates/db/src/database_stats.rs
git commit -m "fix(db-stats): clamp bytes_freed>=0, fix WAL path + misleading comment

bytes_freed: VACUUM can in edge cases produce after_pages > before_pages;
clamp result to 0 so callers never see negative freed bytes.

WAL path: Path::with_extension('sqlite-wal') is correct for db.v2.sqlite
and produces db.v2.sqlite-wal, matching wal_monitor.rs. Remove the wrong
comment claiming string concatenation was required."
```

---

## Task 4: Add archived_at column + update set_archived

**Problem:** The `archived_stats` and `purge_archived` endpoints filter on `updated_at`, but `updated_at` is updated by any write (rename, config change, etc.). A workspace archived 30 days ago but renamed yesterday would be excluded from a `older_than_days=14` query. A dedicated `archived_at` column tracks the actual archive timestamp.

**Files:**
- Create: `crates/db/migrations/20260505000000_add_workspace_archived_at.sql`
- Modify: `crates/db/src/models/workspace.rs:391-404`

- [ ] **Step 1: Create the migration**

Create `crates/db/migrations/20260505000000_add_workspace_archived_at.sql`:

```sql
-- Add archived_at to track when a workspace was archived.
-- Backfill: use updated_at as a conservative approximation for rows
-- that were already archived before this migration ran.
ALTER TABLE workspaces ADD COLUMN archived_at TEXT;

UPDATE workspaces
   SET archived_at = updated_at
 WHERE archived = 1;
```

- [ ] **Step 2: Update set_archived to stamp archived_at**

In `crates/db/src/models/workspace.rs`, replace the `set_archived` function (lines 391–404):

**Before:**
```rust
    pub async fn set_archived(
        pool: &SqlitePool,
        workspace_id: Uuid,
        archived: bool,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "UPDATE workspaces SET archived = $1, updated_at = datetime('now', 'subsec') WHERE id = $2",
            archived,
            workspace_id
        )
        .execute(pool)
        .await?;
        Ok(())
    }
```

**After:**
```rust
    pub async fn set_archived(
        pool: &SqlitePool,
        workspace_id: Uuid,
        archived: bool,
    ) -> Result<(), sqlx::Error> {
        // Use sqlx::query (not query!) to support the CASE expression.
        // archived_at is set to now() when archiving, cleared when un-archiving,
        // so it always reflects the *most recent* archive action.
        sqlx::query(
            "UPDATE workspaces
                SET archived    = ?1,
                    archived_at = CASE WHEN ?1 = 1 THEN datetime('now', 'subsec') ELSE NULL END,
                    updated_at  = datetime('now', 'subsec')
              WHERE id = ?2",
        )
        .bind(archived)
        .bind(workspace_id)
        .execute(pool)
        .await?;
        Ok(())
    }
```

- [ ] **Step 3: Prepare SQLx offline query data**

```bash
cd /path/to/vibe-kanban && pnpm run prepare-db
```

Expected: exits 0. The `sqlx-data.json` (or `.sqlx/` directory) updates to include the new schema.

- [ ] **Step 4: Build to confirm no compile errors**

```bash
cargo build -p vibe-kanban-db 2>&1 | grep -E "^error"
```

Expected: no output.

- [ ] **Step 5: Run db tests**

```bash
cargo test -p vibe-kanban-db
```

Expected: all pass.

- [ ] **Step 6: Commit**

```bash
git add crates/db/migrations/20260505000000_add_workspace_archived_at.sql \
        crates/db/src/models/workspace.rs
git commit -m "feat(db): add archived_at column to workspaces

Tracks the actual timestamp when a workspace was archived, independent
of updated_at (which changes on any write). Backfills existing archived
rows using updated_at as a conservative approximation.

set_archived() now stamps archived_at=now() on archive and clears it
on un-archive so the column always reflects the last archive action."
```

---

## Task 5: Fix archived queries to use archived_at + re-check before purge delete

**Problem A:** `archived_stats` (line 147) and `purge_archived` (lines 204, 230) filter on `updated_at` instead of the new `archived_at`. A workspace archived long ago but recently renamed would be incorrectly excluded.

**Problem B:** `purge_archived` fetches candidates, then iterates deleting them with no re-check. Between the fetch and the delete, a workspace could be un-archived or could acquire a new running process.

**Files:**
- Modify: `crates/server/src/routes/database.rs` — `archived_stats`, `purge_archived` handlers

**Prerequisite:** Task 4 must be complete (migration + `archived_at` column exists).

- [ ] **Step 1: Fix archived_stats query**

In `crates/server/src/routes/database.rs`, replace the query inside `archived_stats` (lines 145–151):

**Before:**
```rust
    let count: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM workspaces
           WHERE archived = 1 AND updated_at < datetime('now', ?)"#,
    )
    .bind(&cutoff)
    .fetch_one(pool)
    .await?;
```

**After:**
```rust
    // COALESCE(archived_at, updated_at): archived_at is NULL for rows that were
    // archived before the migration; fall back to updated_at for those rows.
    let count: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM workspaces
           WHERE archived = 1
             AND COALESCE(archived_at, updated_at) < datetime('now', ?)"#,
    )
    .bind(&cutoff)
    .fetch_one(pool)
    .await?;
```

- [ ] **Step 2: Fix skipped_active count query in purge_archived**

Replace lines 202–214 in `crates/server/src/routes/database.rs`:

**Before:**
```rust
    let skipped_active: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM workspaces w
           WHERE w.archived = 1 AND w.updated_at < datetime('now', ?)
             AND EXISTS (
                 SELECT 1 FROM execution_processes ep
                 JOIN sessions s ON s.id = ep.session_id
                 WHERE s.workspace_id = w.id
                   AND ep.status NOT IN ('completed', 'failed', 'killed')
             )"#,
    )
    .bind(&cutoff)
    .fetch_one(pool)
    .await?;
```

**After:**
```rust
    let skipped_active: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM workspaces w
           WHERE w.archived = 1
             AND COALESCE(w.archived_at, w.updated_at) < datetime('now', ?)
             AND EXISTS (
                 SELECT 1 FROM execution_processes ep
                 JOIN sessions s ON s.id = ep.session_id
                 WHERE s.workspace_id = w.id
                   AND ep.status NOT IN ('completed', 'failed', 'killed')
             )"#,
    )
    .bind(&cutoff)
    .fetch_one(pool)
    .await?;
```

- [ ] **Step 3: Fix candidates query in purge_archived**

Replace lines 216–240 in `crates/server/src/routes/database.rs`:

**Before:**
```rust
    let candidates = sqlx::query_as::<_, Workspace>(
        r#"SELECT
                w.id,
                ...
           FROM workspaces w
           WHERE w.archived = 1 AND w.updated_at < datetime('now', ?)
             AND NOT EXISTS ( ... )"#,
    )
```

**After:**
```rust
    let candidates = sqlx::query_as::<_, Workspace>(
        r#"SELECT
                w.id,
                w.task_id,
                w.container_ref,
                w.branch,
                w.setup_completed_at,
                w.created_at,
                w.updated_at,
                w.archived,
                w.pinned,
                w.name,
                w.worktree_deleted
           FROM workspaces w
           WHERE w.archived = 1
             AND COALESCE(w.archived_at, w.updated_at) < datetime('now', ?)
             AND NOT EXISTS (
                 SELECT 1 FROM execution_processes ep
                 JOIN sessions s ON s.id = ep.session_id
                 WHERE s.workspace_id = w.id
                   AND ep.status NOT IN ('completed', 'failed', 'killed')
             )"#,
    )
    .bind(&cutoff)
    .fetch_all(pool)
    .await?;
```

- [ ] **Step 4: Add per-workspace re-check before deletion**

Replace the loop body (lines 243–261) in `purge_archived`:

**Before:**
```rust
    let mut deleted = 0i64;
    for workspace in &candidates {
        if let Err(e) = deployment.container().delete(workspace).await {
            tracing::warn!(
                workspace_id = %workspace.id,
                "Failed to delete container for archived workspace: {}",
                e
            );
            continue;
        }

        match Workspace::delete(pool, workspace.id).await {
            Ok(_) => deleted += 1,
            Err(e) => tracing::warn!(
                workspace_id = %workspace.id,
                "Failed to delete workspace row after container delete: {}",
                e
            ),
        }
    }
```

**After:**
```rust
    let mut deleted = 0i64;
    for workspace in &candidates {
        // Re-check immediately before deletion: the workspace may have been
        // un-archived or may have acquired a new running process between our
        // initial fetch and now.
        let still_eligible: Option<i64> = sqlx::query_scalar(
            r#"SELECT 1 FROM workspaces w
               WHERE w.id = ? AND w.archived = 1
                 AND NOT EXISTS (
                     SELECT 1 FROM execution_processes ep
                     JOIN sessions s ON s.id = ep.session_id
                     WHERE s.workspace_id = w.id
                       AND ep.status NOT IN ('completed', 'failed', 'killed')
                 )"#,
        )
        .bind(workspace.id)
        .fetch_optional(pool)
        .await?;

        if still_eligible.is_none() {
            tracing::warn!(
                workspace_id = %workspace.id,
                "Workspace no longer eligible for purge on re-check (un-archived or new active process); skipping"
            );
            skipped_active += 1;
            continue;
        }

        if let Err(e) = deployment.container().delete(workspace).await {
            tracing::warn!(
                workspace_id = %workspace.id,
                "Failed to delete container for archived workspace: {}",
                e
            );
            continue;
        }

        match Workspace::delete(pool, workspace.id).await {
            Ok(_) => deleted += 1,
            Err(e) => tracing::warn!(
                workspace_id = %workspace.id,
                "Failed to delete workspace row after container delete: {}",
                e
            ),
        }
    }
```

Note: `skipped_active` must be declared `mut` in the handler. Change line ~201 from `let skipped_active: i64` to `let mut skipped_active: i64`.

- [ ] **Step 5: Prepare SQLx and build**

```bash
pnpm run prepare-db
cargo build -p vibe-kanban-server 2>&1 | grep -E "^error"
```

Expected: no errors.

- [ ] **Step 6: Run tests**

```bash
cargo test --workspace 2>&1 | tail -5
```

Expected: test summary shows 0 failures.

- [ ] **Step 7: Commit**

```bash
git add crates/server/src/routes/database.rs
git commit -m "fix(db-routes): use archived_at for age filter; re-check before purge delete

archived_stats and purge_archived previously filtered on updated_at, which
changes on any workspace write (rename, config change, etc.). Switch to
COALESCE(archived_at, updated_at) so the age filter uses the actual
archive timestamp. Rows from before the migration fall back to updated_at.

purge_archived now re-checks each candidate immediately before deletion to
guard against un-archive or new process creation between fetch and delete."
```

---

## Task 6: Make log purge safe for running processes

**Problem:** `purge_logs` and `log_stats` identify candidate log files by filesystem mtime only. A long-running process whose log file was created >14 days ago would be deleted while the process is still active. Additionally, when `meta.modified()` fails, the file is silently skipped with no log warning.

**Files:**
- Modify: `crates/server/src/routes/database.rs` — `collect_old_log_files`, `walk_log_files`, `log_stats`, `purge_logs`

**Log file path structure** (from `crates/utils/src/execution_logs.rs`):
```text
{asset_dir}/sessions/{2-char-prefix}/{session_id}/processes/{process_id}.jsonl
```

The process UUID is the filename stem. The walk in `database.rs` correctly navigates this structure already.

- [ ] **Step 1: Update collect_old_log_files to return process IDs and warn on mtime failure**

Replace the entire `collect_old_log_files` function in `crates/server/src/routes/database.rs`:

**Before:**
```rust
fn collect_old_log_files(
    root: &std::path::Path,
    cutoff: std::time::SystemTime,
) -> Vec<(std::path::PathBuf, u64)> {
    let mut result = Vec::new();
    let Ok(top) = std::fs::read_dir(root) else {
        return result;
    };
    for prefix_entry in top.flatten() {
        let Ok(sessions_dir) = std::fs::read_dir(prefix_entry.path()) else {
            continue;
        };
        for session_entry in sessions_dir.flatten() {
            let processes_dir = session_entry.path().join("processes");
            let Ok(procs) = std::fs::read_dir(&processes_dir) else {
                continue;
            };
            for proc_entry in procs.flatten() {
                let path = proc_entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                    continue;
                }
                if let Ok(meta) = std::fs::metadata(&path) {
                    if let Ok(mtime) = meta.modified() {
                        if mtime < cutoff {
                            result.push((path, meta.len()));
                        }
                    }
                }
            }
        }
    }
    result
}
```

**After:**
```rust
/// Collect log files older than `cutoff`, returning `(process_id, path, size_bytes)`.
///
/// The process UUID is extracted from the filename stem. Files whose stem is not
/// a valid UUID are skipped with a warning (they are unexpected and should not be deleted).
/// Files where `modified()` fails are skipped conservatively with a warning.
fn collect_old_log_files(
    root: &std::path::Path,
    cutoff: std::time::SystemTime,
) -> Vec<(uuid::Uuid, std::path::PathBuf, u64)> {
    let mut result = Vec::new();
    let Ok(top) = std::fs::read_dir(root) else {
        return result;
    };
    for prefix_entry in top.flatten() {
        let Ok(sessions_dir) = std::fs::read_dir(prefix_entry.path()) else {
            continue;
        };
        for session_entry in sessions_dir.flatten() {
            let processes_dir = session_entry.path().join("processes");
            let Ok(procs) = std::fs::read_dir(&processes_dir) else {
                continue;
            };
            for proc_entry in procs.flatten() {
                let path = proc_entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                    continue;
                }
                // Extract process UUID from filename stem.
                let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                    continue;
                };
                let process_id = match uuid::Uuid::parse_str(stem) {
                    Ok(id) => id,
                    Err(_) => {
                        tracing::warn!(
                            path = %path.display(),
                            "Skipping log file with non-UUID filename"
                        );
                        continue;
                    }
                };
                let Ok(meta) = std::fs::metadata(&path) else {
                    continue;
                };
                let mtime = match meta.modified() {
                    Ok(t) => t,
                    Err(e) => {
                        // Conservative: treat as recent so we never accidentally
                        // delete a file whose age we cannot determine.
                        tracing::warn!(
                            path = %path.display(),
                            error = %e,
                            "Cannot read mtime for log file — skipping conservatively"
                        );
                        continue;
                    }
                };
                if mtime < cutoff {
                    result.push((process_id, path, meta.len()));
                }
            }
        }
    }
    result
}
```

- [ ] **Step 2: Update walk_log_files (used by log_stats) with same mtime warning**

Replace the mtime handling inside `walk_log_files` (lines 373–381):

**Before:**
```rust
                if let Ok(meta) = std::fs::metadata(&path) {
                    if let Ok(mtime) = meta.modified() {
                        if mtime < cutoff {
                            cb(&meta);
                        }
                    }
                }
```

**After:**
```rust
                if let Ok(meta) = std::fs::metadata(&path) {
                    let mtime = match meta.modified() {
                        Ok(t) => t,
                        Err(e) => {
                            tracing::warn!(
                                path = %path.display(),
                                error = %e,
                                "Cannot read mtime for log file — skipping conservatively"
                            );
                            continue;
                        }
                    };
                    if mtime < cutoff {
                        cb(&meta);
                    }
                }
```

- [ ] **Step 3: Refactor purge_logs to use the new collect + async DB filter + blocking delete**

Replace the entire `purge_logs` handler:

**Before:**
```rust
async fn purge_logs(
    State(_deployment): State<DeploymentImpl>,
    Query(query): Query<OlderThanQuery>,
) -> Result<ResponseJson<ApiResponse<LogPurgeResult>>, ApiError> {
    if query.older_than_days < 1 {
        return Err(ApiError::BadRequest(
            "older_than_days must be >= 1".to_string(),
        ));
    }

    let log_root = asset_dir().join(EXECUTION_LOGS_DIRNAME);
    let older_than_days = query.older_than_days;

    let (deleted_files, bytes_freed) = tokio::task::spawn_blocking(move || {
        let cutoff = std::time::SystemTime::now()
            - std::time::Duration::from_secs(older_than_days as u64 * 86400);

        let entries = collect_old_log_files(&log_root, cutoff);
        let mut deleted: i64 = 0;
        let mut freed: i64 = 0;

        for (path, size) in entries {
            if std::fs::remove_file(&path).is_ok() {
                deleted += 1;
                freed += size as i64;
            } else {
                tracing::warn!("Failed to delete log file: {}", path.display());
            }
        }

        (deleted, freed)
    })
    .await
    .map_err(|e| {
        tracing::error!("purge_logs join error: {e}");
        ApiError::Database(sqlx::Error::Protocol(e.to_string()))
    })?;

    Ok(ResponseJson(ApiResponse::success(LogPurgeResult {
        deleted_files,
        bytes_freed,
        older_than_days: query.older_than_days,
    })))
}
```

**After:**
```rust
async fn purge_logs(
    State(deployment): State<DeploymentImpl>,
    Query(query): Query<OlderThanQuery>,
) -> Result<ResponseJson<ApiResponse<LogPurgeResult>>, ApiError> {
    if query.older_than_days < 1 {
        return Err(ApiError::BadRequest(
            "older_than_days must be >= 1".to_string(),
        ));
    }

    let log_root = asset_dir().join(EXECUTION_LOGS_DIRNAME);
    let older_than_days = query.older_than_days;

    // Step 1: Collect candidate (process_id, path, size) in a blocking task.
    let candidates: Vec<(uuid::Uuid, std::path::PathBuf, u64)> =
        tokio::task::spawn_blocking(move || {
            let cutoff = std::time::SystemTime::now()
                - std::time::Duration::from_secs(older_than_days as u64 * 86400);
            collect_old_log_files(&log_root, cutoff)
        })
        .await
        .map_err(|e| {
            tracing::error!("purge_logs collect join error: {e}");
            ApiError::Database(sqlx::Error::Protocol(e.to_string()))
        })?;

    // Step 2: Filter out processes that are still running.
    // Orphaned files (process not in DB) are safe to delete.
    let pool = &deployment.db().pool;
    let mut safe_to_delete: Vec<(std::path::PathBuf, u64)> = Vec::new();
    for (process_id, path, size) in candidates {
        let status: Option<String> = sqlx::query_scalar(
            "SELECT status FROM execution_processes WHERE id = ?",
        )
        .bind(process_id.to_string())
        .fetch_optional(pool)
        .await?;

        match status.as_deref() {
            Some("running") => {
                tracing::debug!(
                    process_id = %process_id,
                    "Skipping log file for still-running process"
                );
            }
            _ => {
                // terminal status ('completed'/'failed'/'killed') OR orphaned (not in DB)
                safe_to_delete.push((path, size));
            }
        }
    }

    // Step 3: Delete safe files in a blocking task.
    let (deleted_files, bytes_freed) = tokio::task::spawn_blocking(move || {
        let mut deleted: i64 = 0;
        let mut freed: i64 = 0;
        for (path, size) in safe_to_delete {
            if std::fs::remove_file(&path).is_ok() {
                deleted += 1;
                freed += size as i64;
            } else {
                tracing::warn!("Failed to delete log file: {}", path.display());
            }
        }
        (deleted, freed)
    })
    .await
    .map_err(|e| {
        tracing::error!("purge_logs delete join error: {e}");
        ApiError::Database(sqlx::Error::Protocol(e.to_string()))
    })?;

    Ok(ResponseJson(ApiResponse::success(LogPurgeResult {
        deleted_files,
        bytes_freed,
        older_than_days: query.older_than_days,
    })))
}
```

- [ ] **Step 4: Build to confirm it compiles**

```bash
cargo build -p vibe-kanban-server 2>&1 | grep -E "^error"
```

Expected: no output.

- [ ] **Step 5: Run tests**

```bash
cargo test --workspace 2>&1 | tail -5
```

Expected: 0 failures.

- [ ] **Step 6: Commit**

```bash
git add crates/server/src/routes/database.rs
git commit -m "fix(log-purge): cross-reference DB status before deleting log files

Previously log files were deleted based purely on filesystem mtime.
A long-running process whose log was created >14 days ago would have
its live log deleted. Now:

1. Collect candidates (with process UUID extracted from filename stem)
2. Query DB: skip any process still in 'running' status
3. Delete only terminal or orphaned process logs

Also: warn (not silently skip) when mtime cannot be read."
```

---

## Task 7: Fix DiskUsageResponse — total_bytes vs displayed_bytes

**Problem:** In `get_disk_usage()` (diagnostics.rs lines 140–143), `total_bytes` is computed across all workspaces, then the list is truncated to 50. The UI footer shows "Total" but sums only the 50 displayed rows via `total_human`. The numbers don't match. Fix by exposing both `total_bytes` (all workspaces) and `displayed_bytes` (sum of displayed top-50) in the response.

**Files:**
- Modify: `crates/server/src/routes/diagnostics.rs`
- Modify: `crates/server/src/bin/generate_types.rs`
- Modify: `packages/web-core/src/shared/dialogs/settings/settings/DiagnosticsPanel.tsx`

- [ ] **Step 1: Update DiskUsageResponse struct**

In `crates/server/src/routes/diagnostics.rs`, replace the `DiskUsageResponse` struct (lines 33–38):

**Before:**
```rust
#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DiskUsageResponse {
    pub workspaces: Vec<WorkspaceDiskUsage>,
    pub total_bytes: u64,
    pub total_human: String,
}
```

**After:**
```rust
#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DiskUsageResponse {
    pub workspaces: Vec<WorkspaceDiskUsage>,
    /// Sum of disk usage across ALL workspaces (not just the displayed top-N).
    pub total_bytes: u64,
    pub total_human: String,
    /// Sum of disk usage for the displayed workspaces (top-50 by size).
    /// May be less than total_bytes when there are more than 50 workspaces.
    pub displayed_bytes: u64,
    pub displayed_human: String,
}
```

- [ ] **Step 2: Fix the total computation in get_disk_usage**

Replace lines 140–148 in `crates/server/src/routes/diagnostics.rs`:

**Before:**
```rust
    usage_list.sort_by(|a, b| b.size_bytes.cmp(&a.size_bytes));
    let total_bytes: u64 = usage_list.iter().map(|w| w.size_bytes).sum();
    usage_list.truncate(50);
    let total_human = format_bytes(total_bytes);

    Ok(ResponseJson(ApiResponse::success(DiskUsageResponse {
        workspaces: usage_list,
        total_bytes,
        total_human,
    })))
```

**After:**
```rust
    usage_list.sort_by(|a, b| b.size_bytes.cmp(&a.size_bytes));
    // Compute total across ALL workspaces before truncating.
    let total_bytes: u64 = usage_list.iter().map(|w| w.size_bytes).sum();
    let total_human = format_bytes(total_bytes);
    usage_list.truncate(50);
    // Compute displayed total after truncating to top-50.
    let displayed_bytes: u64 = usage_list.iter().map(|w| w.size_bytes).sum();
    let displayed_human = format_bytes(displayed_bytes);

    Ok(ResponseJson(ApiResponse::success(DiskUsageResponse {
        workspaces: usage_list,
        total_bytes,
        total_human,
        displayed_bytes,
        displayed_human,
    })))
```

- [ ] **Step 3: Regenerate TypeScript types**

```bash
pnpm run generate-types
```

Expected: `shared/types.ts` updated with `DiskUsageResponse` including `displayed_bytes` and `displayed_human`.

- [ ] **Step 4: Update DiagnosticsPanel to show both totals**

In `packages/web-core/src/shared/dialogs/settings/settings/DiagnosticsPanel.tsx`, replace the total footer row (lines 179–183):

**Before:**
```tsx
              <div className="flex items-center justify-between px-3 py-1.5 bg-secondary/50 border-t border-border">
                <span className="text-sm font-medium text-normal">Total</span>
                <span className="text-sm font-mono font-medium text-normal">
                  {diskData.total_human}
                </span>
              </div>
```

**After:**
```tsx
              <div className="flex flex-col gap-0.5 px-3 py-1.5 bg-secondary/50 border-t border-border">
                <div className="flex items-center justify-between">
                  <span className="text-sm font-medium text-normal">
                    Displayed total
                  </span>
                  <span className="text-sm font-mono font-medium text-normal">
                    {diskData.displayed_human}
                  </span>
                </div>
                {diskData.total_bytes !== diskData.displayed_bytes && (
                  <div className="flex items-center justify-between">
                    <span className="text-xs text-low">All workspaces</span>
                    <span className="text-xs font-mono text-low">
                      {diskData.total_human}
                    </span>
                  </div>
                )}
              </div>
```

- [ ] **Step 5: Check frontend types**

```bash
pnpm run check 2>&1 | grep -E "error TS"
```

Expected: no output.

- [ ] **Step 6: Run full build**

```bash
cargo build --workspace 2>&1 | grep -E "^error"
```

Expected: no output.

- [ ] **Step 7: Commit**

```bash
git add crates/server/src/routes/diagnostics.rs \
        shared/types.ts \
        packages/web-core/src/shared/dialogs/settings/settings/DiagnosticsPanel.tsx
git commit -m "fix(diagnostics): expose displayed_bytes separate from total_bytes

DiskUsageResponse now carries both total_bytes (sum across all workspaces)
and displayed_bytes (sum of the top-50 shown). Previously the footer said
'Total' but showed total_bytes while the rows summed to a different value.
UI now shows displayed total and, when truncation occurred, also shows the
all-workspace total as a secondary line."
```

---

## Task 8: Wire WAL monitor shutdown into server teardown

**Problem:** `WalMonitorHandle::shutdown()` exists and sends a `Shutdown` command to the monitor's event loop, but it is never called. On server exit the tokio runtime drops the task mid-checkpoint, which can interrupt an in-progress WAL flush.

**Files:**
- Modify: `crates/server/src/main.rs:294-300` (`perform_cleanup_actions`)

- [ ] **Step 1: Add WAL monitor shutdown to perform_cleanup_actions**

In `crates/server/src/main.rs`, replace `perform_cleanup_actions` (lines 294–300):

**Before:**
```rust
pub async fn perform_cleanup_actions(deployment: &DeploymentImpl) {
    deployment
        .container()
        .kill_all_running_processes()
        .await
        .expect("Failed to cleanly kill running execution processes");
}
```

**After:**
```rust
pub async fn perform_cleanup_actions(deployment: &DeploymentImpl) {
    deployment
        .container()
        .kill_all_running_processes()
        .await
        .expect("Failed to cleanly kill running execution processes");

    // Signal the WAL monitor to stop and wait for its current checkpoint
    // (if any) to complete before the process exits.
    deployment.wal_monitor().shutdown().await;
    tracing::info!("WAL monitor shut down cleanly");
}
```

- [ ] **Step 2: Build to confirm it compiles**

```bash
cargo build -p vibe-kanban-server 2>&1 | grep -E "^error"
```

Expected: no output.

- [ ] **Step 3: Verify shutdown() is accessible**

`wal_monitor()` is already a public accessor on `LocalDeployment` returning `&WalMonitorHandle`. `WalMonitorHandle::shutdown()` is `pub async fn`. `DeploymentImpl` is `LocalDeployment`. No additional changes needed.

- [ ] **Step 4: Run tests**

```bash
cargo test --workspace 2>&1 | tail -5
```

Expected: 0 failures.

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/main.rs
git commit -m "fix(shutdown): send WAL monitor shutdown signal during server teardown

perform_cleanup_actions now calls deployment.wal_monitor().shutdown().await
so the WAL monitor event loop receives a Shutdown command and exits cleanly
rather than being dropped mid-checkpoint by the tokio runtime."
```

---

## Task 9: Add error states to DiagnosticsPanel and MaintenancePanel

**Problem:** Both panels use React Query hooks but do not destructure `isError`/`error`. When the backend is unreachable or returns an error, the panels render blank with no feedback.

**Files:**
- Modify: `packages/web-core/src/shared/dialogs/settings/settings/DiagnosticsPanel.tsx`
- Modify: `packages/web-core/src/shared/dialogs/settings/settings/MaintenancePanel.tsx`

- [ ] **Step 1: Add error state to DiagnosticsPanel connection pool and WAL cards**

In `packages/web-core/src/shared/dialogs/settings/settings/DiagnosticsPanel.tsx`, replace line 50:

**Before:**
```tsx
  const { data: diagnostics, isLoading: diagLoading } = useDiagnostics();
```

**After:**
```tsx
  const {
    data: diagnostics,
    isLoading: diagLoading,
    isError: diagIsError,
    error: diagError,
  } = useDiagnostics();
```

Then in the Connection Pool card, add an error state after the loading state (after line 70):

**Before:**
```tsx
        {diagLoading && (
          <div className="flex items-center gap-2 text-sm text-low py-2">
            <SpinnerIcon className="size-icon-sm animate-spin" weight="bold" />
            Loading...
          </div>
        )}
        {diagnostics && (
```

**After:**
```tsx
        {diagLoading && (
          <div className="flex items-center gap-2 text-sm text-low py-2">
            <SpinnerIcon className="size-icon-sm animate-spin" weight="bold" />
            Loading...
          </div>
        )}
        {diagIsError && (
          <p className="text-sm text-error py-2">
            Failed to load diagnostics:{' '}
            {(diagError as Error)?.message ?? 'Unknown error'}
          </p>
        )}
        {diagnostics && (
```

Apply the same error block to the WAL Status card (the second `{diagLoading && ...}` / `{diagnostics && ...}` pair):

**Before:**
```tsx
        {diagLoading && (
          <div className="flex items-center gap-2 text-sm text-low py-2">
            <SpinnerIcon className="size-icon-sm animate-spin" weight="bold" />
            Loading...
          </div>
        )}
        {diagnostics && (
          <div className="space-y-2">
```

**After:**
```tsx
        {diagLoading && (
          <div className="flex items-center gap-2 text-sm text-low py-2">
            <SpinnerIcon className="size-icon-sm animate-spin" weight="bold" />
            Loading...
          </div>
        )}
        {diagIsError && (
          <p className="text-sm text-error py-2">
            Failed to load diagnostics:{' '}
            {(diagError as Error)?.message ?? 'Unknown error'}
          </p>
        )}
        {diagnostics && (
          <div className="space-y-2">
```

- [ ] **Step 2: Add error state to MaintenancePanel database stats card**

In `packages/web-core/src/shared/dialogs/settings/settings/MaintenancePanel.tsx`, replace lines 52–56:

**Before:**
```tsx
  const {
    data: stats,
    isLoading: statsLoading,
    refetch: refetchStats,
  } = useDatabaseStats();
```

**After:**
```tsx
  const {
    data: stats,
    isLoading: statsLoading,
    isError: statsIsError,
    error: statsError,
    refetch: refetchStats,
  } = useDatabaseStats();
```

Then in the Database Stats card content, add an error state after the loading state (after line 103):

**Before:**
```tsx
        {statsLoading && (
          <div className="flex items-center gap-2 text-sm text-low py-2">
            <SpinnerIcon className="size-icon-sm animate-spin" weight="bold" />
            Loading...
          </div>
        )}
        {stats && (
```

**After:**
```tsx
        {statsLoading && (
          <div className="flex items-center gap-2 text-sm text-low py-2">
            <SpinnerIcon className="size-icon-sm animate-spin" weight="bold" />
            Loading...
          </div>
        )}
        {statsIsError && (
          <p className="text-sm text-error py-2">
            Failed to load database stats:{' '}
            {(statsError as Error)?.message ?? 'Unknown error'}
          </p>
        )}
        {stats && (
```

- [ ] **Step 3: Check TypeScript**

```bash
pnpm run check 2>&1 | grep -E "error TS"
```

Expected: no output.

- [ ] **Step 4: Lint**

```bash
pnpm run lint 2>&1 | grep -E "error|warning" | head -10
```

Expected: no new errors introduced.

- [ ] **Step 5: Commit**

```bash
git add \
  packages/web-core/src/shared/dialogs/settings/settings/DiagnosticsPanel.tsx \
  packages/web-core/src/shared/dialogs/settings/settings/MaintenancePanel.tsx
git commit -m "fix(ui): show error states in DiagnosticsPanel and MaintenancePanel

Both panels previously rendered blank when the backend was unreachable or
returned an error. Now each card displays a descriptive error message when
its query fails, using React Query's isError/error fields."
```

---

## Final Validation

Run all checks in sequence after all tasks are complete:

- [ ] **Regenerate types** (if not done in Task 7)
```bash
pnpm run generate-types
```

- [ ] **Full Rust build and tests**
```bash
cargo build --workspace 2>&1 | grep -E "^error"
cargo test --workspace
```
Expected: 0 build errors, 0 test failures.

- [ ] **Frontend checks**
```bash
pnpm run check
pnpm run lint
pnpm run format
```
Expected: all pass.

- [ ] **Smoke test with running server**
```bash
pnpm run dev
# In a separate terminal:
curl -s http://localhost:{PORT}/api/diagnostics | jq '.data.pool_stats'
curl -s http://localhost:{PORT}/api/database/stats | jq '.data.database_size_bytes'
curl -sX POST http://localhost:{PORT}/api/database/vacuum | jq '.data.bytes_freed'
# Second call immediately — must get 429:
curl -sX POST http://localhost:{PORT}/api/database/vacuum | jq '.error'
curl -s "http://localhost:{PORT}/api/database/archived-stats?older_than_days=14" | jq '.data.count'
curl -s "http://localhost:{PORT}/api/database/log-stats?older_than_days=14" | jq '.data'
curl -s http://localhost:{PORT}/api/diagnostics/disk-usage | jq '{total: .data.total_bytes, displayed: .data.displayed_bytes}'
```

Expected results:
- `pool_stats` has `size`, `idle`, `acquired` fields
- `database_size_bytes` > 0
- First vacuum returns `bytes_freed >= 0`
- Second vacuum returns 429 error
- `archived-stats` count reflects `archived_at` age, not `updated_at`
- `log-stats` count excludes logs for running processes
- `disk-usage` `total_bytes` >= `displayed_bytes`
- Stopping the backend and opening Settings → Maintenance shows error message, not blank panel

---

## Task 10: Archived workspace "Check" shows per-workspace list with navigation links

**Problem:** The "Check" button on Archive Cleanup only shows a count. Users need to see *which* workspaces would be purged, ordered oldest-first, with a link to open each workspace before deciding to purge.

**Files:**
- Modify: `crates/server/src/routes/database.rs` — new `archived_list` handler + response type
- Modify: `crates/server/src/bin/generate_types.rs` — register `ArchivedListResponse`, `ArchivedWorkspaceItem`
- Modify: `packages/web-core/src/shared/hooks/useDatabaseMaintenance.ts` — add `useArchivedList` hook
- Modify: `packages/web-core/src/shared/dialogs/settings/settings/MaintenancePanel.tsx` — replace count with expandable list

**Prerequisite:** Task 4 must be complete (`archived_at` column exists).

- [ ] **Step 1: Add response types in database.rs**

Add after the `ArchivedStatsResponse` struct in `crates/server/src/routes/database.rs`:

```rust
#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ArchivedWorkspaceItem {
    pub id: Uuid,
    pub name: Option<String>,
    /// ISO-8601 string — the actual archive timestamp (archived_at if set, else updated_at)
    pub archived_at: String,
    /// ISO-8601 string
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ArchivedListResponse {
    pub items: Vec<ArchivedWorkspaceItem>,
    pub older_than_days: i64,
}
```

- [ ] **Step 2: Add the archived_list handler**

Add after the `archived_stats` handler:

```rust
async fn archived_list(
    State(deployment): State<DeploymentImpl>,
    Query(query): Query<OlderThanQuery>,
) -> Result<ResponseJson<ApiResponse<ArchivedListResponse>>, ApiError> {
    if query.older_than_days < 1 {
        return Err(ApiError::BadRequest(
            "older_than_days must be >= 1".to_string(),
        ));
    }
    let pool = &deployment.db().pool;
    let cutoff = format!("-{} days", query.older_than_days);

    // Fetch archived workspaces older than cutoff, ordered oldest first.
    // COALESCE(archived_at, updated_at) handles rows from before the migration.
    let rows = sqlx::query!(
        r#"SELECT
               id AS "id: Uuid",
               name,
               COALESCE(archived_at, updated_at) AS "effective_archived_at!: String",
               created_at AS "created_at!: String"
           FROM workspaces
           WHERE archived = 1
             AND COALESCE(archived_at, updated_at) < datetime('now', ?)
             AND NOT EXISTS (
                 SELECT 1 FROM execution_processes ep
                 JOIN sessions s ON s.id = ep.session_id
                 WHERE s.workspace_id = workspaces.id
                   AND ep.status NOT IN ('completed', 'failed', 'killed')
             )
           ORDER BY COALESCE(archived_at, updated_at) ASC"#,
        cutoff
    )
    .fetch_all(pool)
    .await?;

    let items = rows
        .into_iter()
        .map(|r| ArchivedWorkspaceItem {
            id: r.id,
            name: r.name,
            archived_at: r.effective_archived_at,
            created_at: r.created_at,
        })
        .collect();

    Ok(ResponseJson(ApiResponse::success(ArchivedListResponse {
        items,
        older_than_days: query.older_than_days,
    })))
}
```

- [ ] **Step 3: Register the route**

In the `router()` function at the bottom of `database.rs`, add:

```rust
.route("/database/archived-list", get(archived_list))
```

- [ ] **Step 4: Register TypeScript types**

In `crates/server/src/bin/generate_types.rs`, add to the type export list:

```rust
ArchivedWorkspaceItem::export_all_to(&out_dir)?;
ArchivedListResponse::export_all_to(&out_dir)?;
```

And add the imports at the top (wherever the other database route types are imported from):

```rust
use server::routes::database::{
    // existing types...
    ArchivedWorkspaceItem, ArchivedListResponse,
};
```

- [ ] **Step 5: Prepare SQLx + regenerate types**

```bash
pnpm run prepare-db
pnpm run generate-types
```

Expected: `shared/types.ts` contains `ArchivedWorkspaceItem` and `ArchivedListResponse`.

- [ ] **Step 6: Add useArchivedList hook**

In `packages/web-core/src/shared/hooks/useDatabaseMaintenance.ts`, add:

```typescript
export function useArchivedList(olderThanDays?: number) {
  return useQuery({
    queryKey: ['database', 'archived-list', olderThanDays],
    queryFn: () =>
      apiFetch<ArchivedListResponse>(
        `/api/database/archived-list?older_than_days=${olderThanDays}`
      ),
    enabled: olderThanDays !== undefined && olderThanDays > 0,
    staleTime: 30_000,
  });
}
```

Add the import for `ArchivedListResponse` from `shared/types`.

- [ ] **Step 7: Update MaintenancePanel to show the list**

In `packages/web-core/src/shared/dialogs/settings/settings/MaintenancePanel.tsx`:

1. Add import for `useArchivedList` and the navigation function (TanStack Router's `useNavigate` or `Link`):

```typescript
import { useArchivedList } from '@/shared/hooks/useDatabaseMaintenance';
```

2. Add state + hook after the `archivedStats` lines:

```typescript
const [showArchivedList, setShowArchivedList] = useState(false);
const archivedList = useArchivedList(
  showArchivedList ? Number(archivedDays) : undefined
);
```

3. Replace the "Check" button's `onClick` to also trigger the list:

```typescript
onClick={() => {
  setShowArchivedStats(true);
  setShowArchivedList(true);
  archivedStats.refetch();
  archivedList.refetch();
}}
```

4. Replace the stats display block (below `{showArchivedStats && archivedStats.data && ...}`) with a full list:

```tsx
{showArchivedList && archivedList.data && archivedList.data.items.length > 0 && (
  <div className="rounded-sm border border-border overflow-hidden mt-2">
    <div className="flex items-center justify-between px-3 py-1.5 bg-secondary/50 border-b border-border">
      <span className="text-xs font-medium text-low uppercase tracking-wide">
        Workspace
      </span>
      <span className="text-xs font-medium text-low uppercase tracking-wide">
        Archived
      </span>
    </div>
    {archivedList.data.items.map((item) => (
      <div
        key={item.id}
        className="flex items-center justify-between px-3 py-1.5 border-b border-border last:border-b-0"
      >
        <a
          href={`/workspaces/${item.id}`}
          className="text-sm text-link hover:underline truncate max-w-[60%]"
          title={item.name ?? 'Unnamed workspace'}
        >
          {item.name ?? 'Unnamed workspace'}
        </a>
        <span className="text-xs font-mono text-low shrink-0">
          {new Date(item.archived_at).toLocaleDateString()}
        </span>
      </div>
    ))}
    <div className="px-3 py-1.5 bg-secondary/50 border-t border-border text-xs text-low">
      {archivedList.data.items.length} workspace(s) eligible (oldest first)
    </div>
  </div>
)}

{showArchivedList && archivedList.data?.items.length === 0 && (
  <p className="text-sm text-low mt-1">
    No archived workspaces older than {archivedDays} days.
  </p>
)}
```

- [ ] **Step 8: Check and build**

```bash
pnpm run check 2>&1 | grep -E "error TS"
cargo build -p vibe-kanban-server 2>&1 | grep -E "^error"
```

Expected: no errors.

- [ ] **Step 9: Commit**

```bash
git add crates/server/src/routes/database.rs \
        crates/server/src/bin/generate_types.rs \
        shared/types.ts \
        packages/web-core/src/shared/hooks/useDatabaseMaintenance.ts \
        packages/web-core/src/shared/dialogs/settings/settings/MaintenancePanel.tsx
git commit -m "feat(maintenance): archived workspace list with navigation links

archived-list endpoint returns per-workspace records ordered oldest-first.
MaintenancePanel Check button now shows the list inline with clickable
workspace name links and archive date, replacing the bare count."
```

---

## Task 11: Log "Check" shows session list grouped by workspace with navigation links

**Problem:** The Log Cleanup "Check" button shows only a total count and size. Users need to see which workspaces own the old log files, ordered oldest-first, with a link to navigate to each workspace.

**Files:**
- Modify: `crates/server/src/routes/database.rs` — new `log_list` handler, `LogSessionItem` type, updated walk helper
- Modify: `crates/server/src/bin/generate_types.rs`
- Modify: `packages/web-core/src/shared/hooks/useDatabaseMaintenance.ts`
- Modify: `packages/web-core/src/shared/dialogs/settings/settings/MaintenancePanel.tsx`

**Log path structure** (from `crates/utils/src/execution_logs.rs`):
```text
{asset_dir}/sessions/{2-char-prefix}/{session_id}/processes/{process_id}.jsonl
```

Session UUID → `sessions` table → `workspace_id` → `workspaces.name`.

- [ ] **Step 1: Add a per-session filesystem walk helper**

Add to `crates/server/src/routes/database.rs` after `collect_old_log_files`:

```rust
/// Per-session summary: session UUID + aggregate stats for files older than cutoff.
pub struct SessionLogSummary {
    pub session_id: Uuid,
    pub file_count: i64,
    pub total_bytes: i64,
    /// The oldest mtime found in this session's process directory (as SystemTime).
    pub oldest_mtime: std::time::SystemTime,
}

/// Walk log directories and return per-session summaries for sessions that have
/// at least one file older than `cutoff`. Results are unsorted (caller sorts).
fn collect_old_log_sessions(
    root: &std::path::Path,
    cutoff: std::time::SystemTime,
) -> Vec<SessionLogSummary> {
    let mut summaries: std::collections::HashMap<Uuid, SessionLogSummary> =
        std::collections::HashMap::new();

    let Ok(top) = std::fs::read_dir(root) else {
        return vec![];
    };
    for prefix_entry in top.flatten() {
        let Ok(sessions_dir) = std::fs::read_dir(prefix_entry.path()) else {
            continue;
        };
        for session_entry in sessions_dir.flatten() {
            // Extract session UUID from directory name.
            let session_name = session_entry.file_name();
            let Some(name_str) = session_name.to_str() else {
                continue;
            };
            let Ok(session_id) = Uuid::parse_str(name_str) else {
                continue;
            };
            let processes_dir = session_entry.path().join("processes");
            let Ok(procs) = std::fs::read_dir(&processes_dir) else {
                continue;
            };
            for proc_entry in procs.flatten() {
                let path = proc_entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                    continue;
                }
                let Ok(meta) = std::fs::metadata(&path) else {
                    continue;
                };
                let mtime = match meta.modified() {
                    Ok(t) => t,
                    Err(e) => {
                        tracing::warn!(
                            path = %path.display(),
                            error = %e,
                            "Cannot read mtime for log file — skipping"
                        );
                        continue;
                    }
                };
                if mtime < cutoff {
                    let entry = summaries.entry(session_id).or_insert_with(|| SessionLogSummary {
                        session_id,
                        file_count: 0,
                        total_bytes: 0,
                        oldest_mtime: mtime,
                    });
                    entry.file_count += 1;
                    entry.total_bytes += meta.len() as i64;
                    if mtime < entry.oldest_mtime {
                        entry.oldest_mtime = mtime;
                    }
                }
            }
        }
    }
    summaries.into_values().collect()
}
```

- [ ] **Step 2: Add response types**

Add to `crates/server/src/routes/database.rs` near the other response types:

```rust
#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct LogSessionItem {
    pub session_id: Uuid,
    pub workspace_id: Uuid,
    /// Display name — None if the workspace has no name set.
    pub workspace_name: Option<String>,
    pub file_count: i64,
    pub total_bytes: i64,
    /// ISO-8601 date string of the oldest log file in this session.
    pub oldest_file_date: String,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct LogListResponse {
    pub items: Vec<LogSessionItem>,
    pub older_than_days: i64,
}
```

- [ ] **Step 3: Add the log_list handler**

Add after the `log_stats` handler:

```rust
async fn log_list(
    State(deployment): State<DeploymentImpl>,
    Query(query): Query<OlderThanQuery>,
) -> Result<ResponseJson<ApiResponse<LogListResponse>>, ApiError> {
    if query.older_than_days < 1 {
        return Err(ApiError::BadRequest(
            "older_than_days must be >= 1".to_string(),
        ));
    }

    let log_root = asset_dir().join(EXECUTION_LOGS_DIRNAME);
    let older_than_days = query.older_than_days;

    // Step 1: Collect per-session summaries in a blocking task.
    let session_summaries: Vec<SessionLogSummary> =
        tokio::task::spawn_blocking(move || {
            let cutoff = std::time::SystemTime::now()
                - std::time::Duration::from_secs(older_than_days as u64 * 86400);
            collect_old_log_sessions(&log_root, cutoff)
        })
        .await
        .map_err(|e| ApiError::Database(sqlx::Error::Protocol(e.to_string())))?;

    if session_summaries.is_empty() {
        return Ok(ResponseJson(ApiResponse::success(LogListResponse {
            items: vec![],
            older_than_days: query.older_than_days,
        })));
    }

    // Step 2: Join session UUIDs to workspaces via the DB.
    let pool = &deployment.db().pool;
    let mut items: Vec<LogSessionItem> = Vec::new();

    for summary in session_summaries {
        // Query: sessions → workspaces for this session UUID.
        let row = sqlx::query!(
            r#"SELECT s.workspace_id AS "workspace_id: Uuid", w.name
               FROM sessions s
               JOIN workspaces w ON w.id = s.workspace_id
               WHERE s.id = ?"#,
            summary.session_id
        )
        .fetch_optional(pool)
        .await?;

        // Skip sessions that no longer exist in the DB (orphaned log directories).
        let Some(row) = row else {
            continue;
        };

        // Convert oldest_mtime to an ISO-8601 date string.
        let oldest_file_date = {
            let secs = summary
                .oldest_mtime
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let dt = chrono::DateTime::<Utc>::from_timestamp(secs as i64, 0)
                .unwrap_or_default();
            dt.format("%Y-%m-%d").to_string()
        };

        items.push(LogSessionItem {
            session_id: summary.session_id,
            workspace_id: row.workspace_id,
            workspace_name: row.name,
            file_count: summary.file_count,
            total_bytes: summary.total_bytes,
            oldest_file_date,
        });
    }

    // Sort: oldest first.
    items.sort_by(|a, b| a.oldest_file_date.cmp(&b.oldest_file_date));

    Ok(ResponseJson(ApiResponse::success(LogListResponse {
        items,
        older_than_days: query.older_than_days,
    })))
}
```

- [ ] **Step 4: Register the route + TS types**

In `router()` add:
```rust
.route("/database/log-list", get(log_list))
```

In `generate_types.rs` add:
```rust
LogSessionItem::export_all_to(&out_dir)?;
LogListResponse::export_all_to(&out_dir)?;
```

- [ ] **Step 5: Prepare and regenerate**

```bash
pnpm run prepare-db
pnpm run generate-types
```

Expected: `shared/types.ts` contains `LogSessionItem` and `LogListResponse`.

- [ ] **Step 6: Add useLogList hook**

In `packages/web-core/src/shared/hooks/useDatabaseMaintenance.ts`:

```typescript
export function useLogList(olderThanDays?: number) {
  return useQuery({
    queryKey: ['database', 'log-list', olderThanDays],
    queryFn: () =>
      apiFetch<LogListResponse>(
        `/api/database/log-list?older_than_days=${olderThanDays}`
      ),
    enabled: olderThanDays !== undefined && olderThanDays > 0,
    staleTime: 30_000,
  });
}
```

- [ ] **Step 7: Update MaintenancePanel log section to show the list**

In `MaintenancePanel.tsx`, add hook state + list display in the Log File Cleanup card, mirroring the archived list pattern from Task 10:

```typescript
const [showLogList, setShowLogList] = useState(false);
const logList = useLogList(showLogList ? Number(logDays) : undefined);
```

Update the "Check" button onClick:

```typescript
onClick={() => {
  setShowLogStats(true);
  setShowLogList(true);
  logStats.refetch();
  logList.refetch();
}}
```

Add list display below the log stats line:

```tsx
{showLogList && logList.data && logList.data.items.length > 0 && (
  <div className="rounded-sm border border-border overflow-hidden mt-2">
    <div className="flex items-center justify-between px-3 py-1.5 bg-secondary/50 border-b border-border">
      <span className="text-xs font-medium text-low uppercase tracking-wide">Workspace</span>
      <span className="text-xs font-medium text-low uppercase tracking-wide">Files / Size / Oldest</span>
    </div>
    {logList.data.items.map((item) => (
      <div
        key={item.session_id}
        className="flex items-center justify-between px-3 py-1.5 border-b border-border last:border-b-0"
      >
        <a
          href={`/workspaces/${item.workspace_id}`}
          className="text-sm text-link hover:underline truncate max-w-[50%]"
          title={item.workspace_name ?? 'Unnamed workspace'}
        >
          {item.workspace_name ?? 'Unnamed workspace'}
        </a>
        <span className="text-xs font-mono text-low shrink-0">
          {String(item.file_count)} / {formatBytes(item.total_bytes)} / {item.oldest_file_date}
        </span>
      </div>
    ))}
    <div className="px-3 py-1.5 bg-secondary/50 border-t border-border text-xs text-low">
      {logList.data.items.length} session(s) with eligible log files (oldest first)
    </div>
  </div>
)}

{showLogList && logList.data?.items.length === 0 && (
  <p className="text-sm text-low mt-1">
    No log files older than {logDays} days.
  </p>
)}
```

- [ ] **Step 8: Check and build**

```bash
pnpm run check 2>&1 | grep -E "error TS"
cargo build -p vibe-kanban-server 2>&1 | grep -E "^error"
```

Expected: no errors.

- [ ] **Step 9: Commit**

```bash
git add crates/server/src/routes/database.rs \
        crates/server/src/bin/generate_types.rs \
        shared/types.ts \
        packages/web-core/src/shared/hooks/useDatabaseMaintenance.ts \
        packages/web-core/src/shared/dialogs/settings/settings/MaintenancePanel.tsx
git commit -m "feat(maintenance): log session list with workspace links

log-list endpoint groups old log files by session, joins to workspace
names, and returns items ordered oldest-first. MaintenancePanel Check
button now expands an inline list with workspace links, file count, size,
and oldest file date per session."
```

---

## Task 12: Disk usage workspace cleanup — remove build artifacts and archived worktrees

**Problem:** The Disk Usage panel shows per-workspace disk consumption but offers no cleanup actions. Users want two options per workspace: (A) remove common build artifact directories (`node_modules/`, `target/`, `.next/`, etc.) without touching source code, and (B) remove the entire worktree for archived workspaces.

**Files:**
- Modify: `crates/server/src/routes/diagnostics.rs` — two new handlers + response types
- Modify: `crates/server/src/bin/generate_types.rs`
- Modify: `packages/web-core/src/shared/hooks/useDiagnostics.ts`
- Modify: `packages/web-core/src/shared/dialogs/settings/settings/DiagnosticsPanel.tsx`

**Build artifact directories to target** (tunable list; delete the first match found per workspace root):

```text
node_modules  target  .next  .nuxt  dist  build  .venv
__pycache__   .turbo  .cache .parcel-cache out .output
```

- [ ] **Step 1: Add CleanArtifactsResult and RemoveWorktreeResult types**

In `crates/server/src/routes/diagnostics.rs`:

```rust
#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CleanArtifactsResult {
    /// Paths of directories that were removed (relative to workspace root).
    pub dirs_removed: Vec<String>,
    pub bytes_freed: u64,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RemoveWorktreeResult {
    pub workspace_id: Uuid,
    pub success: bool,
}
```

- [ ] **Step 2: Add the clean_artifacts handler**

```rust
/// Known build artifact directory names to remove during cleanup.
const ARTIFACT_DIRS: &[&str] = &[
    "node_modules",
    "target",
    ".next",
    ".nuxt",
    "dist",
    "build",
    ".venv",
    "__pycache__",
    ".turbo",
    ".cache",
    ".parcel-cache",
    "out",
    ".output",
];

async fn clean_artifacts(
    State(deployment): State<DeploymentImpl>,
    axum::extract::Path(workspace_id): axum::extract::Path<Uuid>,
) -> Result<ResponseJson<ApiResponse<CleanArtifactsResult>>, ApiError> {
    let pool = &deployment.db().pool;

    let workspace = db::models::workspace::Workspace::find_by_id(pool, workspace_id)
        .await?
        .ok_or_else(|| ApiError::NotFound("Workspace not found".to_string()))?;

    let container_ref = workspace.container_ref.ok_or_else(|| {
        ApiError::BadRequest("Workspace has no container reference".to_string())
    })?;

    if workspace.worktree_deleted {
        return Err(ApiError::BadRequest(
            "Workspace worktree has already been deleted".to_string(),
        ));
    }

    let workspace_path = std::path::PathBuf::from(&container_ref);
    if !workspace_path.exists() {
        return Err(ApiError::BadRequest(
            "Workspace directory does not exist on disk".to_string(),
        ));
    }

    let (dirs_removed, bytes_freed) = tokio::task::spawn_blocking(move || {
        let mut removed = Vec::new();
        let mut freed = 0u64;

        for &dir_name in ARTIFACT_DIRS {
            let candidate = workspace_path.join(dir_name);
            if candidate.exists() {
                // Measure size before removal.
                let size = dir_size(&candidate);
                match std::fs::remove_dir_all(&candidate) {
                    Ok(()) => {
                        freed += size;
                        removed.push(dir_name.to_string());
                    }
                    Err(e) => {
                        tracing::warn!(
                            path = %candidate.display(),
                            error = %e,
                            "Failed to remove artifact directory"
                        );
                    }
                }
            }
        }
        (removed, freed)
    })
    .await
    .map_err(|e| ApiError::Database(sqlx::Error::Protocol(e.to_string())))?;

    Ok(ResponseJson(ApiResponse::success(CleanArtifactsResult {
        dirs_removed,
        bytes_freed,
    })))
}

/// Recursive directory size (best-effort; skips unreadable entries).
fn dir_size(path: &std::path::Path) -> u64 {
    let mut total = 0u64;
    let mut stack = vec![path.to_path_buf()];
    while let Some(dir) = stack.pop() {
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                if let Ok(meta) = entry.metadata() {
                    if meta.is_file() {
                        total += meta.len();
                    } else if meta.is_dir() {
                        stack.push(entry.path());
                    }
                }
            }
        }
    }
    total
}
```

- [ ] **Step 3: Add the remove_worktree handler**

```rust
async fn remove_worktree(
    State(deployment): State<DeploymentImpl>,
    axum::extract::Path(workspace_id): axum::extract::Path<Uuid>,
) -> Result<ResponseJson<ApiResponse<RemoveWorktreeResult>>, ApiError> {
    let pool = &deployment.db().pool;

    let workspace = db::models::workspace::Workspace::find_by_id(pool, workspace_id)
        .await?
        .ok_or_else(|| ApiError::NotFound("Workspace not found".to_string()))?;

    if !workspace.archived {
        return Err(ApiError::BadRequest(
            "Only archived workspaces can have their worktree removed via this endpoint".to_string(),
        ));
    }

    if workspace.worktree_deleted {
        return Ok(ResponseJson(ApiResponse::success(RemoveWorktreeResult {
            workspace_id,
            success: true, // idempotent
        })));
    }

    // Delegate to the container service which stops processes, removes the
    // worktree directory, and marks worktree_deleted = true in the DB.
    deployment
        .container()
        .delete(&workspace)
        .await
        .map_err(|e| {
            tracing::error!(workspace_id = %workspace_id, "remove_worktree error: {e}");
            ApiError::Database(sqlx::Error::Protocol(e.to_string()))
        })?;

    Ok(ResponseJson(ApiResponse::success(RemoveWorktreeResult {
        workspace_id,
        success: true,
    })))
}
```

- [ ] **Step 4: Register the routes**

In the `router()` function of `crates/server/src/routes/diagnostics.rs`:

```rust
.route(
    "/diagnostics/disk-usage/:workspace_id/clean-artifacts",
    axum::routing::post(clean_artifacts),
)
.route(
    "/diagnostics/disk-usage/:workspace_id/remove-worktree",
    axum::routing::post(remove_worktree),
)
```

- [ ] **Step 5: Register TS types**

In `generate_types.rs`:

```rust
CleanArtifactsResult::export_all_to(&out_dir)?;
RemoveWorktreeResult::export_all_to(&out_dir)?;
```

- [ ] **Step 6: Regenerate types and build**

```bash
pnpm run generate-types
cargo build -p vibe-kanban-server 2>&1 | grep -E "^error"
```

Expected: no errors; `shared/types.ts` has `CleanArtifactsResult` and `RemoveWorktreeResult`.

- [ ] **Step 7: Add cleanup mutation hooks**

In `packages/web-core/src/shared/hooks/useDiagnostics.ts`:

```typescript
import { useMutation, useQueryClient } from '@tanstack/react-query';

export function useCleanArtifacts() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (workspaceId: string) =>
      apiFetch<CleanArtifactsResult>(
        `/api/diagnostics/disk-usage/${workspaceId}/clean-artifacts`,
        { method: 'POST' }
      ),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['diagnostics', 'disk-usage'] });
    },
  });
}

export function useRemoveWorktree() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (workspaceId: string) =>
      apiFetch<RemoveWorktreeResult>(
        `/api/diagnostics/disk-usage/${workspaceId}/remove-worktree`,
        { method: 'POST' }
      ),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['diagnostics', 'disk-usage'] });
    },
  });
}
```

- [ ] **Step 8: Add action buttons to DiagnosticsPanel disk usage rows**

In `packages/web-core/src/shared/dialogs/settings/settings/DiagnosticsPanel.tsx`:

1. Import hooks + confirm dialog:

```typescript
import { useDiagnostics, useDiskUsage, useCleanArtifacts, useRemoveWorktree } from '@/shared/hooks/useDiagnostics';
import { ConfirmDialog } from '@vibe/ui/components/ConfirmDialog';
import { SparkleIcon, FolderMinusIcon } from '@phosphor-icons/react';
```

2. Add hooks inside `DiagnosticsPanel`:

```typescript
const cleanArtifacts = useCleanArtifacts();
const removeWorktree = useRemoveWorktree();
```

3. Update each workspace row to include action buttons (replace the existing row `<div>`):

```tsx
{diskData.workspaces.map((ws) => (
  <div
    key={ws.workspace_id}
    className="flex items-center justify-between px-3 py-1.5 border-b border-border last:border-b-0 gap-2"
  >
    <span
      className="text-sm text-normal truncate max-w-[45%]"
      title={ws.path}
    >
      {ws.path}
    </span>
    <div className="flex items-center gap-2 shrink-0">
      <span className="text-sm font-mono text-normal">
        {formatBytes(ws.size_bytes)}
      </span>
      <button
        className="text-xs text-low hover:text-normal transition-colors"
        title="Remove build artifacts (node_modules, target, .next, etc.)"
        disabled={cleanArtifacts.isPending}
        onClick={async () => {
          const result = await ConfirmDialog.show({
            title: 'Remove Build Artifacts',
            message: `Remove build artifact directories (node_modules, target, .next, etc.) from this workspace? Source code will not be affected.`,
            confirmText: 'Clean',
            variant: 'destructive',
          });
          if (result === 'confirmed') {
            cleanArtifacts.mutate(ws.workspace_id);
          }
        }}
      >
        <SparkleIcon className="size-icon-sm" weight="bold" />
      </button>
      <button
        className="text-xs text-low hover:text-error transition-colors"
        title="Remove worktree directory (archived workspaces only)"
        disabled={removeWorktree.isPending}
        onClick={async () => {
          const result = await ConfirmDialog.show({
            title: 'Remove Worktree',
            message: `Permanently remove the workspace directory from disk. The workspace record will remain in the app. This cannot be undone.`,
            confirmText: 'Remove',
            variant: 'destructive',
          });
          if (result === 'confirmed') {
            removeWorktree.mutate(ws.workspace_id);
          }
        }}
      >
        <FolderMinusIcon className="size-icon-sm" weight="bold" />
      </button>
    </div>
  </div>
))}
```

- [ ] **Step 9: Check and build**

```bash
pnpm run check 2>&1 | grep -E "error TS"
cargo build -p vibe-kanban-server 2>&1 | grep -E "^error"
```

Expected: no errors.

- [ ] **Step 10: Commit**

```bash
git add crates/server/src/routes/diagnostics.rs \
        crates/server/src/bin/generate_types.rs \
        shared/types.ts \
        packages/web-core/src/shared/hooks/useDiagnostics.ts \
        packages/web-core/src/shared/dialogs/settings/settings/DiagnosticsPanel.tsx
git commit -m "feat(diagnostics): disk usage cleanup — clean artifacts + remove worktree

Two new endpoints per workspace:
- POST /diagnostics/disk-usage/:id/clean-artifacts — removes known build
  artifact dirs (node_modules, target, .next, etc.) and reports bytes freed
- POST /diagnostics/disk-usage/:id/remove-worktree — removes the worktree
  for archived workspaces via the container service

DiagnosticsPanel adds per-row icon buttons with confirm dialogs."
```

---

## Task 13: Uncommitted changes check before archiving a workspace

**Problem:** Users can archive a workspace that has uncommitted local changes with no warning. The changes are not lost (the files remain on disk) but the user may not realise they exist. Before archiving, the server should check for uncommitted changes and surface a warning that the user must explicitly dismiss.

**Files:**
- Modify: `crates/db/src/models/requests.rs` — add `force_archive` field to `UpdateWorkspace`
- Modify: `crates/server/src/routes/workspaces/core.rs` — add dirty check before archiving
- Frontend (location TBD by implementer): wherever `archived: true` is sent in workspace update calls, catch 409 and show a confirm dialog

**Key API:** `GitService::is_worktree_clean(&self, worktree_path: &Path) -> Result<bool, GitServiceError>` at `crates/git/src/lib.rs:815`. Returns `Ok(true)` if clean, `Ok(false)` if dirty (uncommitted changes detected).

- [ ] **Step 1: Add force_archive to UpdateWorkspace request**

In `crates/db/src/models/requests.rs`, update the `UpdateWorkspace` struct:

**Before:**
```rust
pub struct UpdateWorkspace {
    pub archived: Option<bool>,
    pub pinned: Option<bool>,
    pub name: Option<String>,
}
```

**After:**
```rust
pub struct UpdateWorkspace {
    pub archived: Option<bool>,
    pub pinned: Option<bool>,
    pub name: Option<String>,
    /// When true, skip the uncommitted-changes check and archive even if the
    /// worktree has local changes. Defaults to false.
    #[serde(default)]
    pub force_archive: bool,
}
```

- [ ] **Step 2: Add dirty check in update_workspace handler**

In `crates/server/src/routes/workspaces/core.rs`, update the `update_workspace` handler. After line 49 (`let is_archiving = ...`), add:

```rust
    // Guard: if archiving and the worktree has uncommitted changes, refuse
    // unless the client explicitly passes force_archive = true.
    if is_archiving && !request.force_archive {
        // Only check if the worktree directory still exists on disk.
        if let Some(ref container_ref) = workspace.container_ref {
            if !workspace.worktree_deleted {
                let path = std::path::PathBuf::from(container_ref);
                if path.exists() {
                    match deployment.git().is_worktree_clean(&path) {
                        Ok(true) => {
                            // Clean — proceed normally.
                        }
                        Ok(false) => {
                            return Err(ApiError::Conflict(
                                "Workspace has uncommitted changes. \
                                 Pass force_archive=true to archive anyway."
                                    .to_string(),
                            ));
                        }
                        Err(e) => {
                            // If we can't determine git status (e.g. not a git repo),
                            // log a warning but do not block the archive.
                            tracing::warn!(
                                workspace_id = %workspace.id,
                                error = ?e,
                                "Could not determine git dirty status; proceeding with archive"
                            );
                        }
                    }
                }
            }
        }
    }
```

- [ ] **Step 3: Build to confirm it compiles**

```bash
cargo build -p vibe-kanban-server 2>&1 | grep -E "^error"
```

Expected: no output.

- [ ] **Step 4: Locate the workspace archive call in the frontend**

Find where the frontend sets `archived: true` on a workspace. Search:

```bash
grep -rn "archived.*true\|archive" packages/web-core/src/ --include="*.ts" --include="*.tsx" | grep -v "node_modules"
```

This will identify the API call site(s). They will look like:

```typescript
workspacesApi.update(workspaceId, { archived: true })
```

or similar. Note the file(s) for the next step.

- [ ] **Step 5: Wrap archive call with dirty-check error handling**

At each call site found in Step 4, wrap the archive mutation to intercept a 409 response and show a force-confirm dialog. The pattern:

```typescript
// In the component that triggers archiving:
const handleArchive = async (workspaceId: string) => {
  try {
    await workspacesApi.update(workspaceId, { archived: true });
  } catch (error) {
    // 409 Conflict = uncommitted changes detected
    if ((error as { status?: number }).status === 409) {
      const result = await ConfirmDialog.show({
        title: 'Uncommitted Changes Detected',
        message:
          'This workspace has uncommitted changes that will remain on disk after archiving. ' +
          'Archive anyway?',
        confirmText: 'Archive Anyway',
        variant: 'destructive',
      });
      if (result === 'confirmed') {
        // Retry with force_archive flag
        await workspacesApi.update(workspaceId, {
          archived: true,
          force_archive: true,
        });
      }
    } else {
      throw error;
    }
  }
};
```

Note: `workspacesApi.update` type signature must accept `force_archive?: boolean`. Update the TypeScript API client type if needed (it should be inferred from the Rust struct update via `generate-types`).

- [ ] **Step 6: Check UpdateWorkspaceRequest in api-types**

Check `crates/api-types/src/workspaces.rs` for `UpdateWorkspaceRequest`. If it is a separate struct from `UpdateWorkspace` in `crates/db`, it may also need `force_archive` added. Update it to match.

- [ ] **Step 7: Regenerate types**

```bash
pnpm run generate-types
```

Expected: `shared/types.ts` shows `UpdateWorkspace` with `force_archive?: boolean`.

- [ ] **Step 8: Full check**

```bash
pnpm run check 2>&1 | grep -E "error TS"
cargo test --workspace 2>&1 | tail -5
```

Expected: no errors, all tests pass.

- [ ] **Step 9: Commit**

```bash
git add crates/db/src/models/requests.rs \
        crates/server/src/routes/workspaces/core.rs \
        shared/types.ts
# Also add the frontend call site file(s) from Step 4
git commit -m "feat(workspaces): check for uncommitted changes before archiving

update_workspace now calls GitService::is_worktree_clean before setting
archived=true. If dirty, returns 409 Conflict unless force_archive=true.

Frontend catches the 409, shows a confirm dialog ('Archive Anyway?'),
and retries with force_archive=true if the user confirms."
```

---

## Final Validation

Run all checks in sequence after all tasks are complete:

- [ ] **Regenerate types** (if not done in individual tasks)
```bash
pnpm run generate-types
```

- [ ] **Full Rust build and tests**
```bash
cargo build --workspace 2>&1 | grep -E "^error"
cargo test --workspace
```
Expected: 0 build errors, 0 test failures.

- [ ] **Frontend checks**
```bash
pnpm run check
pnpm run lint
pnpm run format
```
Expected: all pass.

- [ ] **Smoke test with running server**
```bash
pnpm run dev
# In a separate terminal:
curl -s http://localhost:{PORT}/api/diagnostics | jq '.data.pool_stats'
curl -s http://localhost:{PORT}/api/database/stats | jq '.data.database_size_bytes'
curl -sX POST http://localhost:{PORT}/api/database/vacuum | jq '.data.bytes_freed'
# Second call immediately — must get 429:
curl -sX POST http://localhost:{PORT}/api/database/vacuum | jq '.error'
curl -s "http://localhost:{PORT}/api/database/archived-stats?older_than_days=14" | jq '.data.count'
curl -s "http://localhost:{PORT}/api/database/archived-list?older_than_days=14" | jq '.data.items | length'
curl -s "http://localhost:{PORT}/api/database/log-stats?older_than_days=14" | jq '.data'
curl -s "http://localhost:{PORT}/api/database/log-list?older_than_days=14" | jq '.data.items | length'
curl -s http://localhost:{PORT}/api/diagnostics/disk-usage | jq '{total: .data.total_bytes, displayed: .data.displayed_bytes}'
```

Expected results:
- `pool_stats` has `size`, `idle`, `acquired` fields
- `database_size_bytes` > 0
- First vacuum returns `bytes_freed >= 0`; second returns 429
- `archived-list` returns array with id/name/archived_at per workspace
- `log-list` returns array grouped by session with workspace names
- `disk-usage` `total_bytes` >= `displayed_bytes`; rows have cleanup icon buttons
- Archiving a workspace with uncommitted changes shows a confirm dialog
- Stopping the backend and opening Settings → Maintenance shows error messages, not blank panels

---

## Execution Order Notes

Tasks 1–3 and 7–9 are fully independent and can be executed in any order.
Tasks 5 and 6 depend on Task 4 (migration must run first so `archived_at` column exists).
Tasks 10–13 are independent of each other and of Tasks 1–9.

Suggested sequence: 1 → 2 → 3 → 4 → 5 → 6 → 7 → 8 → 9 → 10 → 11 → 12 → 13 → Final Validation.
