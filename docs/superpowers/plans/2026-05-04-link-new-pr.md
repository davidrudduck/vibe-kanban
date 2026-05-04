# Link New PR Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Allow a workspace to link a new PR after its previously linked PR has been merged or closed.

**Architecture:** The sole change is in `attach_existing_pr` — extract the "is an open PR already attached?" guard into a pure function `find_open_pr_merge`, replace the faulty `.next()` guard that blocked on any PR status with one that only blocks on `MergeStatus::Open`, and unit-test the extracted function inline following the pattern already established in `links.rs`.

**Tech Stack:** Rust, axum, SQLx (SQLite), `db::models::merge::{Merge, MergeStatus, PrMerge, PullRequestInfo}`

---

## File Map

| File | Change |
|------|--------|
| `crates/server/src/routes/workspaces/pr.rs` | Extract `find_open_pr_merge`, fix guard, add `#[cfg(test)]` module |

No other files require changes. The UI visibility predicate (`!ctx.hasOpenPR` in `packages/web-core/src/shared/actions/index.ts:1013`) is already correct — it's already false when the existing PR is merged.

---

## Task 1: Extract `find_open_pr_merge` and fix the guard

**Files:**
- Modify: `crates/server/src/routes/workspaces/pr.rs:407-416`

### Context

`attach_existing_pr` (line 391) currently returns early for **any** PR already attached to the workspace/repo pair, regardless of status. The call at line 408 returns all PRs (`open`, `merged`, `closed`) ordered `created_at DESC`, then `.next()` takes the most recent one. If that PR is merged, the handler short-circuits and never queries the git host for a new open PR.

The fix: replace `.next()` with `.find(...)` that filters for `MergeStatus::Open` only. To keep the logic unit-testable (following the `classify_remote_workspace_sync` pattern in `links.rs`), extract the predicate into a private function.

---

- [ ] **Step 1: Add the `find_open_pr_merge` helper function**

Open `crates/server/src/routes/workspaces/pr.rs`. After the last `use` import block (around line 39) and before the first struct definition, add:

```rust
/// Returns the first open PR merge for the given list of merges, or `None` if
/// no open PR is attached. Ignores merged, closed, and direct merges so that a
/// new PR can be linked after the previous one has been merged/closed.
fn find_open_pr_merge(merges: Vec<Merge>) -> Option<PrMerge> {
    merges.into_iter().find_map(|m| {
        if let Merge::Pr(p) = m {
            if matches!(p.pr_info.status, MergeStatus::Open) {
                return Some(p);
            }
        }
        None
    })
}
```

- [ ] **Step 2: Replace the faulty guard in `attach_existing_pr`**

Locate lines 407–416 in `attach_existing_pr`. Replace:

```rust
    // Check if PR already attached for this repo
    let merges = Merge::find_by_workspace_and_repo_id(pool, workspace.id, request.repo_id).await?;
    if let Some(Merge::Pr(pr_merge)) = merges.into_iter().next() {
        return Ok(ResponseJson(ApiResponse::success(AttachPrResponse {
            pr_attached: true,
            pr_url: Some(pr_merge.pr_info.url.clone()),
            pr_number: Some(pr_merge.pr_info.number),
            pr_status: Some(pr_merge.pr_info.status.clone()),
        })));
    }
```

With:

```rust
    // Only short-circuit if an *open* PR is already attached.
    // A merged or closed PR should allow a new PR to be linked.
    let merges = Merge::find_by_workspace_and_repo_id(pool, workspace.id, request.repo_id).await?;
    if let Some(pr_merge) = find_open_pr_merge(merges) {
        return Ok(ResponseJson(ApiResponse::success(AttachPrResponse {
            pr_attached: true,
            pr_url: Some(pr_merge.pr_info.url.clone()),
            pr_number: Some(pr_merge.pr_info.number),
            pr_status: Some(pr_merge.pr_info.status.clone()),
        })));
    }
```

- [ ] **Step 3: Verify it compiles**

```bash
cd /path/to/vibe-kanban && cargo check -p server
```

Expected: no errors. If you see "cannot find value `pr_merge` in this scope" you have a leftover `Merge::Pr(pr_merge)` binding — remove it (the new code binds `pr_merge` directly from `find_open_pr_merge`).

---

## Task 2: Unit tests for `find_open_pr_merge`

**Files:**
- Modify: `crates/server/src/routes/workspaces/pr.rs` (add `#[cfg(test)]` module at end of file)

These are pure synchronous tests — no async runtime, no database. The same pattern is used in `crates/server/src/routes/workspaces/links.rs:234-301`.

- [ ] **Step 1: Write the failing tests**

Append to the bottom of `crates/server/src/routes/workspaces/pr.rs`:

```rust
#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::*;

    fn make_pr_merge_with_status(status: MergeStatus) -> Merge {
        Merge::Pr(PrMerge {
            id: Uuid::new_v4(),
            workspace_id: Uuid::new_v4(),
            repo_id: Uuid::new_v4(),
            created_at: Utc::now(),
            target_branch_name: "main".to_string(),
            pr_info: PullRequestInfo {
                number: 1,
                url: "https://github.com/owner/repo/pull/1".to_string(),
                status,
                merged_at: None,
                merge_commit_sha: None,
            },
        })
    }

    fn make_direct_merge() -> Merge {
        Merge::Direct(DirectMerge {
            id: Uuid::new_v4(),
            workspace_id: Uuid::new_v4(),
            repo_id: Uuid::new_v4(),
            merge_commit: "abc123".to_string(),
            target_branch_name: "main".to_string(),
            merge_strategy: "merge".to_string(),
            created_at: Utc::now(),
        })
    }

    #[test]
    fn find_open_pr_merge_returns_none_for_empty_list() {
        assert!(find_open_pr_merge(vec![]).is_none());
    }

    #[test]
    fn find_open_pr_merge_returns_none_when_only_merged_pr() {
        // This is the key regression case: a merged PR must NOT block new PR linking.
        let merges = vec![make_pr_merge_with_status(MergeStatus::Merged)];
        assert!(find_open_pr_merge(merges).is_none());
    }

    #[test]
    fn find_open_pr_merge_returns_none_when_only_closed_pr() {
        let merges = vec![make_pr_merge_with_status(MergeStatus::Closed)];
        assert!(find_open_pr_merge(merges).is_none());
    }

    #[test]
    fn find_open_pr_merge_returns_none_when_only_unknown_pr() {
        let merges = vec![make_pr_merge_with_status(MergeStatus::Unknown)];
        assert!(find_open_pr_merge(merges).is_none());
    }

    #[test]
    fn find_open_pr_merge_returns_none_for_direct_merge_only() {
        let merges = vec![make_direct_merge()];
        assert!(find_open_pr_merge(merges).is_none());
    }

    #[test]
    fn find_open_pr_merge_returns_open_pr() {
        let merges = vec![make_pr_merge_with_status(MergeStatus::Open)];
        let result = find_open_pr_merge(merges);
        assert!(result.is_some());
        assert!(matches!(result.unwrap().pr_info.status, MergeStatus::Open));
    }

    #[test]
    fn find_open_pr_merge_returns_open_pr_when_mixed_with_merged() {
        // Simulates: old merged PR + new open PR both in the list.
        let merges = vec![
            make_pr_merge_with_status(MergeStatus::Merged),
            make_pr_merge_with_status(MergeStatus::Open),
        ];
        let result = find_open_pr_merge(merges);
        assert!(result.is_some());
        assert!(matches!(result.unwrap().pr_info.status, MergeStatus::Open));
    }

    #[test]
    fn find_open_pr_merge_ignores_direct_merges_even_with_open_pr() {
        let merges = vec![
            make_direct_merge(),
            make_pr_merge_with_status(MergeStatus::Open),
        ];
        let result = find_open_pr_merge(merges);
        assert!(result.is_some());
    }
}
```

- [ ] **Step 2: Run the tests — expect them to FAIL (function not yet extracted)**

If you added the tests before Task 1, they should fail to compile. If Task 1 is done first they will pass — that's fine, the important thing is to run them.

```bash
cargo test -p server find_open_pr_merge 2>&1 | head -40
```

Expected after Task 1 is complete: all 8 tests pass.

- [ ] **Step 3: Run full workspace test suite to confirm no regressions**

```bash
cargo test --workspace 2>&1 | tail -20
```

Expected: all tests pass, no failures.

- [ ] **Step 4: Run lint**

```bash
pnpm run lint 2>&1 | tail -30
```

Expected: no new warnings or errors. If clippy warns about `find_map` simplification, apply the suggestion.

- [ ] **Step 5: Format**

```bash
pnpm run format
```

Expected: exits 0. Commit only if format changed files.

- [ ] **Step 6: Commit**

```bash
git add crates/server/src/routes/workspaces/pr.rs
git commit -m "fix: allow linking a new PR after the previous PR is merged

Extract find_open_pr_merge helper so the attach_existing_pr guard only
blocks when an open PR is already attached. A merged or closed PR no
longer prevents a new PR from being linked to the workspace."
```

---

## Self-Review Checklist

**Spec coverage:**
- ✅ If a PR is merged, "Link PR" action calls `attach_existing_pr` → Task 1 fixes the guard so it no longer short-circuits on merged PRs
- ✅ If an open PR is already linked, behavior is unchanged — guard still returns early
- ✅ `create_for_workspace` uses `ON CONFLICT(pr_url) DO UPDATE` so re-linking the same URL is safe (no duplicate rows)
- ✅ Workspace archival logic at lines 508–526 is unaffected — it only runs when a newly attached PR is already merged, which is a separate branch of the handler

**No placeholders:** All code blocks are complete and self-contained.

**Type consistency:**
- `find_open_pr_merge(merges: Vec<Merge>) -> Option<PrMerge>` — consistent across Task 1 (definition) and Task 2 (tests call `find_open_pr_merge(merges).is_none()` / `.is_some()`)
- `make_pr_merge_with_status` returns `Merge` (not `PrMerge`) — matches `Vec<Merge>` parameter ✅
- `PrMerge`, `PullRequestInfo`, `DirectMerge`, `MergeStatus` all imported via `use super::*` in the test module ✅
