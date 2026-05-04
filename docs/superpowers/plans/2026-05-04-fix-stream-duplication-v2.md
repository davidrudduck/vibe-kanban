# Fix Chat Stream Duplication — Remediation Plan (v2)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close the four root-cause gaps left open after the v1 fix (`settledStreamProcessIdsRef`), then verify the full fix eliminates duplication on all paths: normal completion, user-initiated kill, stream reconnect, and scope navigation.

**Why v1 is incomplete:** The v1 guard works only when the backend emits the `Finished` log frame before flipping the process status in the DB. This ordering is reliable for natural agent completion but is **guaranteed to fail** for user-initiated kills — `stop_execution` in `container.rs` calls `update_completion` (line 1595) before `push_finished()` (line 1641), with up to 5 s of graceful-shutdown wait in between. In that window, Effect C fires and re-fetches, producing a duplicate. Additionally, the WebSocket close handler in `streamJsonPatchEntries.ts` does not signal failure on an unclean close, leaving the Promise hanging and making the backoff retry loop unable to distinguish success from silent failure.

---

## Files changed

| File | Task |
|------|------|
| `crates/local-deployment/src/container.rs` | Task 1 — move `update_completion` after `push_finished()` |
| `packages/web-core/src/shared/lib/streamJsonPatchEntries.ts` | Task 2 — add `finishedReceived` flag; signal error on unclean close |
| `packages/web-core/src/features/workspace-chat/model/hooks/useConversationHistory.ts` | Task 3 — widen Effect C guard; Task 4 — add exhausted-backoff fallback |
| `packages/web-core/src/features/workspace-chat/model/hooks/useConversationHistory.ts` | Task 5 — audit and delete Effect C |

---

## Background: root-cause map

```text
Normal completion (happy path)
  Backend: push_finished() → update_completion()   [correct ordering]
  onFinished fires first → settledStreamProcessIdsRef ✓ → Effect C skipped ✓

User-initiated kill (stop_execution)
  Backend: update_completion() → [0–5 s wait] → push_finished()   [WRONG ordering]
  Effect C fires first → settledStreamProcessIdsRef empty → re-fetch → DUPLICATE ✗

Stream reconnect / backoff retry
  onFinished never fires during retry gaps
  Effect C fires during a gap → settledStreamProcessIdsRef empty → re-fetch → DUPLICATE ✗

Unclean WebSocket close
  close handler does NOT call opts.onError → Promise hangs indefinitely
  loadRunningAndEmitWithBackoff never retries → stream silently stalled ✗
```

---

## Task 1: Fix `stop_execution` — move `update_completion` after `push_finished()`

**File:** `crates/local-deployment/src/container.rs`

**Root cause:** Line 1595 writes the terminal status to the DB immediately, triggering an SQLite update-hook event that notifies the frontend. Lines 1640–1641 call `push_finished()` only after a potential 5 s graceful-shutdown wait. The frontend receives the status-change event first, Effect C fires before `onFinished`, and `settledStreamProcessIdsRef` is still empty.

**Fix:** Move `ExecutionProcess::update_completion` to after `push_finished()` and the DB stream handle drain, so the status event is the last thing the frontend sees — never the first.

- [ ] **Step 1: Read the current `stop_execution` body** (lines 1578–1656) to confirm the exact position of each statement.

- [ ] **Step 2: Reorder `update_completion` to after the DB stream drain**

  Current order (abridged):
  ```rust
  // Line 1595 — DB flip FIRST
  ExecutionProcess::update_completion(&self.db.pool, execution_process.id, status, exit_code).await?;

  // Lines 1604–1622 — graceful cancel + up-to-5-s wait
  // Lines 1625–1635 — force kill
  // Line 1636 — remove child from store

  // Lines 1639–1645 — push_finished() + drain DB stream handle
  let db_stream_handle = self.take_db_stream_handle(&execution_process.id).await;
  if let Some(msg) = self.msg_stores.write().await.remove(&execution_process.id) {
      msg.push_finished();
  }
  if let Some(handle) = db_stream_handle {
      let _ = tokio::time::timeout(Duration::from_secs(5), handle).await;
  }
  ```

  Target order:
  ```rust
  // Release ProtocolPeer (keep in place)
  self.protocol_peers.write().await.remove(&execution_process.id);

  // Graceful cancel + up-to-5-s wait (keep in place)
  // Force kill (keep in place)
  // Remove child from store (keep in place)

  // push_finished() + drain DB stream handle (keep in place)
  let db_stream_handle = self.take_db_stream_handle(&execution_process.id).await;
  if let Some(msg) = self.msg_stores.write().await.remove(&execution_process.id) {
      msg.push_finished();
  }
  if let Some(handle) = db_stream_handle {
      let _ = tokio::time::timeout(Duration::from_secs(5), handle).await;
  }

  // DB flip LAST — status event now always trails the Finished log frame
  ExecutionProcess::update_completion(&self.db.pool, execution_process.id, status, exit_code).await?;
  ```

  **Why safe:** The Finished frame is now persisted to the DB log store (via the drained `db_stream_handle`) before the status flip. The frontend will only see `running → completed/killed` after `onFinished` has fired. The 5-second timeout on the DB stream drain is already in place and bounds the delay.

- [ ] **Step 3: Verify Rust compilation**

  ```bash
  cd /path/to/vibe-kanban && pnpm run backend:check
  ```

  Expected: exits 0, no errors on `crates/local-deployment`.

- [ ] **Step 4: Run Rust tests**

  ```bash
  cargo test -p local-deployment
  ```

  Expected: all tests pass (or no tests in the crate, which is acceptable).

- [ ] **Step 5: Commit**

  ```bash
  git add crates/local-deployment/src/container.rs
  git commit -m "fix(container): flip DB status after push_finished() so Finished frame always precedes status event"
  ```

---

## Task 2: Fix `streamJsonPatchEntries.ts` — signal error on unclean WebSocket close

**File:** `packages/web-core/src/shared/lib/streamJsonPatchEntries.ts`

**Root cause:** The `close` event handler (lines 132–138) cancels the rAF timer but does not check whether a `Finished` frame was received. If the socket closes without a `Finished` frame (network drop, server restart, timeout), the Promise returned by `loadRunningAndEmit` never resolves or rejects — it hangs indefinitely. `loadRunningAndEmitWithBackoff` therefore stalls at `await loadRunningAndEmit(...)` and never retries.

**Fix:** Add a `finishedReceived` boolean flag. In the `close` handler, call `opts.onError` when `!finishedReceived` so the caller's Promise rejects and the backoff loop can retry.

- [ ] **Step 1: Add `finishedReceived` flag**

  In `streamJsonPatchEntries.ts`, immediately after `let closed = false;` (line 47), add:
  ```ts
  let finishedReceived = false;
  ```

- [ ] **Step 2: Set flag when `Finished` message arrives**

  In `handleMessage`, at the top of the `if (msg.finished !== undefined)` block (line 97), add:
  ```ts
  finishedReceived = true;
  ```

  Full block after change:
  ```ts
  if (msg.finished !== undefined) {
    finishedReceived = true;          // ← new
    if (rafId !== null) {
      cancelAnimationFrame(rafId);
    }
    flush();
    opts.onFinished?.(snapshot.entries);
    ws?.close();
  }
  ```

- [ ] **Step 3: Call `opts.onError` on unclean close**

  Replace the current `close` handler (lines 132–138):
  ```ts
  ws.addEventListener('close', () => {
    connected = false;
    if (rafId !== null) {
      cancelAnimationFrame(rafId);
      rafId = null;
    }
  });
  ```

  With:
  ```ts
  ws.addEventListener('close', () => {
    connected = false;
    if (rafId !== null) {
      cancelAnimationFrame(rafId);
      rafId = null;
    }
    if (!finishedReceived && !closed) {
      opts.onError?.(new Error('WebSocket closed without Finished frame'));
    }
  });
  ```

  **Why the `!closed` guard:** When the caller's `controller.close()` runs (e.g., cleanup in `onFinished`), it sets `closed = true` and then calls `ws.close()`. The `close` event fires after that, but we must NOT call `onError` for an intentional close.

- [ ] **Step 4: TypeScript check**

  ```bash
  pnpm run web-core:check
  ```

  Expected: exits 0.

- [ ] **Step 5: Unit test**

  Create `packages/web-core/src/shared/lib/streamJsonPatchEntries.test.ts` with the following cases:

  ```ts
  import { describe, it, expect, vi, beforeEach } from 'vitest';

  // Mock WebSocket
  // ...

  describe('streamJsonPatchEntries', () => {
    it('calls onFinished and does NOT call onError when Finished frame received', async () => { ... });

    it('calls onError when WebSocket closes without Finished frame', async () => { ... });

    it('does NOT call onError when controller.close() closes the socket (intentional close)', async () => { ... });

    it('calls onError on WebSocket error event', async () => { ... });
  });
  ```

  Run tests:
  ```bash
  pnpm --filter web-core run test
  ```

  Expected: all 4 cases pass.

- [ ] **Step 6: Commit**

  ```bash
  git add packages/web-core/src/shared/lib/streamJsonPatchEntries.ts \
          packages/web-core/src/shared/lib/streamJsonPatchEntries.test.ts
  git commit -m "fix(stream): signal onError on unclean WebSocket close so backoff can retry"
  ```

---

## Task 3: Widen Effect C guard — also skip when stream is actively retrying

**File:** `packages/web-core/src/features/workspace-chat/model/hooks/useConversationHistory.ts`

**Root cause:** Effect C's current guard checks `!settledStreamProcessIdsRef.current.has(process.id)`. This only guards against a cleanly-finished stream. If a stream fails and `loadRunningAndEmitWithBackoff` is mid-retry (backoff sleep), `streamingProcessIdsRef` contains the process ID but `settledStreamProcessIdsRef` does not. Effect C fires, fetches historic entries, and emits them. When the retry succeeds, `onFinished` emits again — duplicate.

**Fix:** Add a second guard: `!streamingProcessIdsRef.current.has(process.id)`. If the live stream is still active (or retrying), Effect C defers. The process only falls through to Effect C if: (a) the stream never started for this process, or (b) the stream exhausted all retries without settling (handled by Task 4).

- [ ] **Step 1: Locate Effect C's condition block** (around line 491)

  Current:
  ```ts
  if (
    previousStatus === ExecutionProcessStatus.running &&
    currentStatus !== ExecutionProcessStatus.running &&
    displayedExecutionProcesses.current[process.id] &&
    !settledStreamProcessIdsRef.current.has(process.id)
  ) {
    processesToReload.push(process);
  }
  ```

- [ ] **Step 2: Add the streaming guard**

  Replace with:
  ```ts
  if (
    previousStatus === ExecutionProcessStatus.running &&
    currentStatus !== ExecutionProcessStatus.running &&
    displayedExecutionProcesses.current[process.id] &&
    !settledStreamProcessIdsRef.current.has(process.id) &&
    !streamingProcessIdsRef.current.has(process.id)
  ) {
    processesToReload.push(process);
  }
  ```

  **Why safe:** `streamingProcessIdsRef` is populated the moment Effect B starts a stream (line 462) and cleared in the `.finally()` (line 464). If Effect B is mid-backoff-sleep, the process is still in `streamingProcessIdsRef` because `finally` has not fired yet. Skipping Effect C here is correct — if the retry eventually succeeds, `onFinished` delivers the final state. If all retries exhaust, `finally` runs and `streamingProcessIdsRef.current.delete(process.id)` fires, but by then Effect C has already missed the transition. Task 4 covers that gap with an explicit fallback.

- [ ] **Step 3: TypeScript check**

  ```bash
  pnpm run web-core:check
  ```

  Expected: exits 0.

- [ ] **Step 4: Commit**

  ```bash
  git add packages/web-core/src/features/workspace-chat/model/hooks/useConversationHistory.ts
  git commit -m "fix(chat): guard Effect C against re-fetch while live stream is active or retrying"
  ```

---

## Task 4: Add exhausted-backoff fallback in `loadRunningAndEmitWithBackoff`

**File:** `packages/web-core/src/features/workspace-chat/model/hooks/useConversationHistory.ts`

**Root cause:** After Task 3, Effect C no longer fires for processes that are still streaming (in `streamingProcessIdsRef`). But when all 20 retries exhaust without `onFinished` firing — and the process has already transitioned to non-running — Effect C has missed its chance (the `previousStatus → currentStatus` transition is a one-shot edge in `idStatusKey`). The historic entries are never loaded: blank chat log.

**Fix:** In `loadRunningAndEmitWithBackoff`, after the retry loop exits, check whether the process was settled. If not, check whether the process is now non-running and load its historic entries directly. This makes the fallback explicit and co-located with the backoff logic, rather than relying on Effect C's timing.

- [ ] **Step 1: Locate `loadRunningAndEmitWithBackoff`** (lines 248–260)

  Current:
  ```ts
  const loadRunningAndEmitWithBackoff = useCallback(
    async (executionProcess: ExecutionProcess) => {
      for (let i = 0; i < 20; i++) {
        try {
          await loadRunningAndEmit(executionProcess);
          break;
        } catch (_) {
          await new Promise((resolve) => setTimeout(resolve, 500));
        }
      }
    },
    [loadRunningAndEmit]
  );
  ```

  Note: this callback captures `loadRunningAndEmit` only. The fallback needs `loadEntriesForHistoricExecutionProcess`, `emitEntries`, `displayedExecutionProcesses`, `settledStreamProcessIdsRef`, and `executionProcesses`. Check whether all are already in the hook's lexical scope (they are — they are defined earlier in the same hook body) so they will be captured without changing the dependency array except for `loadEntriesForHistoricExecutionProcess`.

- [ ] **Step 2: Add the exhaustion fallback**

  Replace with:
  ```ts
  const loadRunningAndEmitWithBackoff = useCallback(
    async (executionProcess: ExecutionProcess) => {
      for (let i = 0; i < 20; i++) {
        try {
          await loadRunningAndEmit(executionProcess);
          return; // onFinished fired; stream settled cleanly
        } catch (_) {
          await new Promise((resolve) => setTimeout(resolve, 500));
        }
      }

      // All retries exhausted without a clean Finished frame.
      // If the process has already transitioned to non-running, load its
      // final entries from the historic endpoint as a last resort.
      if (settledStreamProcessIdsRef.current.has(executionProcess.id)) {
        return; // settled concurrently while we were in the last retry sleep
      }

      const currentProcess = executionProcesses.current?.find(
        (p) => p.id === executionProcess.id
      );
      if (
        currentProcess &&
        currentProcess.status !== ExecutionProcessStatus.running
      ) {
        const entries =
          await loadEntriesForHistoricExecutionProcess(currentProcess);
        if (entries.length > 0) {
          const entriesWithKey = entries.map((e, idx) =>
            patchWithKey(e, currentProcess.id, idx)
          );
          mergeIntoDisplayed((state) => {
            state[currentProcess.id] = {
              executionProcess: currentProcess,
              entries: entriesWithKey,
            };
          });
          emitEntries(displayedExecutionProcesses.current, 'running', false);
        }
      }
    },
    [
      loadRunningAndEmit,
      loadEntriesForHistoricExecutionProcess,
      emitEntries,
      executionProcesses,
    ]
  );
  ```

  **Note on `mergeIntoDisplayed`:** Verify that `mergeIntoDisplayed` is available in the hook's scope at the point `loadRunningAndEmitWithBackoff` is defined. If not, use the direct `displayedExecutionProcesses.current[currentProcess.id] = { ... }` assignment pattern used elsewhere in the hook, then call `emitEntries`.

- [ ] **Step 3: TypeScript check**

  ```bash
  pnpm run web-core:check
  ```

  Expected: exits 0. If the `useCallback` dependency array is incomplete, TypeScript's `react-hooks/exhaustive-deps` ESLint rule will flag it — fix any warnings.

- [ ] **Step 4: Lint check**

  ```bash
  pnpm run lint
  ```

  Expected: exits 0, no new warnings.

- [ ] **Step 5: Commit**

  ```bash
  git add packages/web-core/src/features/workspace-chat/model/hooks/useConversationHistory.ts
  git commit -m "fix(chat): load historic entries after backoff exhaustion when stream never settled"
  ```

---

## Task 5: Audit and delete Effect C

**File:** `packages/web-core/src/features/workspace-chat/model/hooks/useConversationHistory.ts`

**Goal:** Determine whether Effect C has any remaining purpose after Tasks 1–4. If it is confirmed dead code, delete it to eliminate the entire class of race conditions it represents.

**Conditions under which Effect C fires (post Tasks 1–4):**

| Scenario | Fires? | Correct outcome |
|----------|--------|----------------|
| Normal completion — `onFinished` before status change (Task 1 guarantees this) | No — `settledStreamProcessIdsRef` guard | ✓ |
| User kill — `onFinished` before status change (Task 1 fixes ordering) | No — `settledStreamProcessIdsRef` guard | ✓ |
| Active stream or backoff retry | No — `streamingProcessIdsRef` guard (Task 3) | ✓ |
| All retries exhausted, process non-running | No — fallback in Task 4 runs first | ✓ |
| Process never had a live stream (e.g., process added to scope while already non-running) | **Maybe** — `displayedExecutionProcesses.current[process.id]` guard blocks if process not yet in display store | See below |

The `displayedExecutionProcesses.current[process.id]` condition in Effect C means it only fires for processes that were already in the display store (i.e., they were seen as running and Effect B had been tracking them). A process that transitioned to non-running before Effect B ever started would never be in the display store — it would be loaded by `loadHistoricEntries` instead. This means Effect C's surviving trigger condition is a narrow edge: process was added to display store, Effect B ran, stream failed after 20 retries, **and** for some reason the Task 4 fallback also did not load entries. This is not a realistic scenario given the Task 4 safety net.

- [ ] **Step 1: Trace every `processesToReload` path and confirm none are reachable post Tasks 1–4**

  Search for any code path that populates `processesToReload` where Tasks 1–4 would not have already loaded the entries.

  ```bash
  grep -n "processesToReload" packages/web-core/src/features/workspace-chat/model/hooks/useConversationHistory.ts
  ```

- [ ] **Step 2: Confirm Effect C is dead code — checklist**

  - [ ] Task 1 guarantees `update_completion` trails `push_finished()` for all stop paths
  - [ ] Task 2 ensures unclean WS close calls `onError`, allowing backoff to retry
  - [ ] Task 3 ensures Effect C does not fire while a stream is active or retrying
  - [ ] Task 4 handles the exhausted-backoff case explicitly inside `loadRunningAndEmitWithBackoff`
  - [ ] No external code path calls `emitEntries` for a `running` process without also setting `streamingProcessIdsRef`

- [ ] **Step 3: If confirmed dead — delete Effect C**

  Remove the entire `useEffect` block at approximately lines 476–530 (the one depending on `[idStatusKey, executionProcessesRaw, emitEntries]` that populates `processesToReload`).

  Also remove `previousStatusMapRef` and its initialisation (line ~50) and its scope-reset clear (line ~389) if it is no longer used elsewhere.

  ```bash
  grep -n "previousStatusMapRef" packages/web-core/src/features/workspace-chat/model/hooks/useConversationHistory.ts
  ```

- [ ] **Step 4: TypeScript and lint check after deletion**

  ```bash
  pnpm run web-core:check && pnpm run lint
  ```

  Expected: exits 0, no unused variable warnings.

- [ ] **Step 5: If NOT confirmed dead — add a deprecation comment instead**

  If any scenario cannot be proven safe, do not delete. Add a comment above Effect C:
  ```ts
  // REVIEW(2026-05-04): Effect C may be dead code after:
  //   - Task 1: container.rs stop_execution ordering fix
  //   - Task 2: streamJsonPatchEntries unclean-close error signalling
  //   - Task 3: streamingProcessIdsRef guard added to this condition
  //   - Task 4: exhausted-backoff fallback in loadRunningAndEmitWithBackoff
  // Retain until confirmed safe to delete via E2E Scenario B validation.
  ```

- [ ] **Step 6: Commit**

  ```bash
  git add packages/web-core/src/features/workspace-chat/model/hooks/useConversationHistory.ts
  git commit -m "fix(chat): remove Effect C — all recovery paths now handled by stream layer and backoff fallback"
  # or, if retained:
  git commit -m "chore(chat): annotate Effect C as candidate for deletion post-v2 remediation"
  ```

---

## Task 6: Format and final build verification

- [ ] **Step 1: Run formatter**

  ```bash
  pnpm run format
  ```

- [ ] **Step 2: Run full check**

  ```bash
  pnpm run check
  ```

  Expected: exits 0 across all frontend TypeScript workspaces and all Rust workspaces.

- [ ] **Step 3: Run lint**

  ```bash
  pnpm run lint
  ```

  Expected: exits 0, no warnings on modified files.

- [ ] **Step 4: Run all tests**

  ```bash
  cargo test --workspace && pnpm --filter web-core run test
  ```

  Expected: all Rust tests pass; new `streamJsonPatchEntries.test.ts` tests pass.

---

## Task 7: Manual E2E validation

**Prerequisites:** `pnpm run dev` running locally.

### Scenario A — Normal completion: no duplication (v1 regression test)

- [ ] Send a message to the coding agent
- [ ] Watch the chat log during streaming
- [ ] Wait for agent to complete
- [ ] **Pass:** each assistant message, tool-use block, and thinking group appears exactly once
- [ ] **Fail:** any content block appears twice, or content flickers then reappears

### Scenario B — User-initiated kill: no duplication (primary new test for v2)

- [ ] Start a task, let the agent begin streaming
- [ ] Click **Stop** / kill the process via the UI
- [ ] **Pass:** chat log shows the partial entries streamed so far — no duplication, no blank log
- [ ] **Fail:** entries duplicated after the kill, or entries disappear

### Scenario C — Forced WebSocket disconnect: fallback recovery works

- [ ] Start a task with the agent running
- [ ] Open DevTools → Network → find the `normalized-logs/ws` WebSocket → close it manually
- [ ] Wait for backoff to exhaust (≈10 s) or for the process to complete
- [ ] **Pass:** final entries load (either via backoff retry or the Task 4 exhaustion fallback)
- [ ] **Fail:** chat log empty or stalled after disconnect

### Scenario D — Multiple sequential agents: no cross-contamination

- [ ] Send message → wait for completion
- [ ] Send follow-up → wait for completion
- [ ] **Pass:** each agent turn shows entries exactly once; no entries bleed between turns
- [ ] **Fail:** entries duplicate or bleed across turns

### Scenario E — Scope navigation: settled IDs cleared correctly

- [ ] Complete a task (agent finishes, entries stable)
- [ ] Navigate to a different task and back
- [ ] **Pass:** entries reload correctly; no duplication
- [ ] **Fail:** entries missing or Effect C blocked by stale settled ID

### Scenario F — Rapid stop/start: no residual state

- [ ] Start a task, immediately stop it before any streaming begins
- [ ] Start a new task in the same scope
- [ ] **Pass:** new agent's entries appear cleanly; no ghost entries from the stopped process
- [ ] **Fail:** stopped process's partial entries reappear or duplicate in the new turn

---

## Summary of changes

```bash
crates/local-deployment/src/container.rs
  Line ~1595: remove update_completion (moved)
  Line ~1645: + ExecutionProcess::update_completion(...).await?;   (+1 line, -0 net)

packages/web-core/src/shared/lib/streamJsonPatchEntries.ts
  Line ~47:  + let finishedReceived = false;
  Line ~97:  + finishedReceived = true;
  Lines ~132–138: close handler — + 3 lines (onError call)        (+5 lines total)

packages/web-core/src/features/workspace-chat/model/hooks/useConversationHistory.ts
  Line ~495: + && !streamingProcessIdsRef.current.has(process.id)  (+1 line)
  Lines ~248–260: loadRunningAndEmitWithBackoff — add fallback     (+~18 lines)
  Lines ~476–530: delete Effect C (if confirmed dead)              (-~55 lines)

packages/web-core/src/shared/lib/streamJsonPatchEntries.test.ts   (new file, ~80 lines)
```

**Net change:** ~25 lines added, ~55 lines removed (if Effect C deleted), 1 new test file.

**Zero new dependencies. Zero new files except the test file.**
