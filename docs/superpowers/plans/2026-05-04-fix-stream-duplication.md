# Fix Chat Stream Duplication Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Eliminate chat log duplication caused by Effect B (live stream) and Effect C (status-transition re-fetch) both emitting final state for the same process when streaming completes.

**Architecture:** Add a `settledStreamProcessIdsRef` to track processes whose live stream ended cleanly via `onFinished`. Guard Effect C so it only re-fetches processes that were NOT already settled by Effect B — making Effect C a true error-recovery fallback rather than a guaranteed duplicate path. Four targeted line additions in one file.

**Tech Stack:** React 18, TypeScript, `useRef`, `useEffect` — no new dependencies.

---

## Background: why duplication happens

When a coding-agent process finishes, two independent code paths both race to write final entries and call `emitEntries`:

1. **Effect B** (`loadRunningAndEmitWithBackoff`, line 437) — receives the live WebSocket stream; calls `emitEntries` inside its `onFinished` callback.
2. **Effect C** (line 473) — watches `idStatusKey`; when any process transitions `running → non-running` it re-opens a *new* WebSocket to the historic endpoint, fetches the same entries, and calls `emitEntries` again.

`onTimelineUpdated`'s rAF coalescing only deduplicates emissions that occur within the same synchronous call stack. Effect B and Effect C fire in different event-loop turns, so each schedules its own `requestAnimationFrame` flush, producing two separate React state updates. The second render may produce different `DisplayEntry` object references (fresh fetch = fresh objects), causing `buildConversationRowsIncremental`'s `entries[i] === prevEntries[i]` reference check to fail on tail rows and create visually duplicate rows.

---

## Files changed

| File | Change |
|------|--------|
| `packages/web-core/src/features/workspace-chat/model/hooks/useConversationHistory.ts` | +4 lines — add ref, mark settled, guard Effect C, clear on scope reset |

No new files. No other files touched.

---

## Task 1: Add `settledStreamProcessIdsRef` declaration

**File:** `packages/web-core/src/features/workspace-chat/model/hooks/useConversationHistory.ts`

- [ ] **Step 1: Locate the ref declarations block**

  Open the file. Find the block starting around line 38 that looks like:

  ```ts
  const executionProcesses = useRef<ExecutionProcess[]>(executionProcessesRaw);
  const displayedExecutionProcesses = useRef<ExecutionProcessStateStore>({});
  const loadedInitialEntries = useRef(false);
  const emittedEmptyInitialRef = useRef(false);
  const streamingProcessIdsRef = useRef<Set<string>>(new Set());
  ```

- [ ] **Step 2: Add the new ref immediately after `streamingProcessIdsRef`**

  Replace:
  ```ts
  const streamingProcessIdsRef = useRef<Set<string>>(new Set());
  ```
  With:
  ```ts
  const streamingProcessIdsRef = useRef<Set<string>>(new Set());
  const settledStreamProcessIdsRef = useRef<Set<string>>(new Set());
  ```

- [ ] **Step 3: Run TypeScript check — expect no errors**

  ```bash
  cd /path/to/vibe-kanban && pnpm run web-core:check
  ```

  Expected output: exits 0, no errors on the modified file.

- [ ] **Step 4: Commit**

  ```bash
  git add packages/web-core/src/features/workspace-chat/model/hooks/useConversationHistory.ts
  git commit -m "fix(chat): add settledStreamProcessIdsRef to track cleanly-finished streams"
  ```

---

## Task 2: Mark a process as settled when its stream `onFinished` fires

**File:** `packages/web-core/src/features/workspace-chat/model/hooks/useConversationHistory.ts`

- [ ] **Step 1: Locate the `loadRunningAndEmit` `onFinished` callback**

  Find the block around line 230 that reads:

  ```ts
  onFinished: () => {
    emitEntries(displayedExecutionProcesses.current, 'running', false);
    controller.close();
    resolve();
  },
  ```

- [ ] **Step 2: Add the settled-mark as the first line of `onFinished`**

  Replace:
  ```ts
  onFinished: () => {
    emitEntries(displayedExecutionProcesses.current, 'running', false);
    controller.close();
    resolve();
  },
  ```
  With:
  ```ts
  onFinished: () => {
    settledStreamProcessIdsRef.current.add(executionProcess.id);
    emitEntries(displayedExecutionProcesses.current, 'running', false);
    controller.close();
    resolve();
  },
  ```

  **Why first?** Effect C's status-change watcher may fire synchronously during the same microtask queue drain as `onFinished`. Marking settled before `emitEntries` ensures the guard in Task 3 is already set before Effect C's `idStatusKey` watcher can execute.

- [ ] **Step 3: Run TypeScript check — expect no errors**

  ```bash
  pnpm run web-core:check
  ```

  Expected output: exits 0.

- [ ] **Step 4: Commit**

  ```bash
  git add packages/web-core/src/features/workspace-chat/model/hooks/useConversationHistory.ts
  git commit -m "fix(chat): mark stream as settled in onFinished before emitting final state"
  ```

---

## Task 3: Guard Effect C — skip re-fetch for settled processes

**File:** `packages/web-core/src/features/workspace-chat/model/hooks/useConversationHistory.ts`

- [ ] **Step 1: Locate Effect C's transition detection condition**

  Find the block around line 482 inside the `useEffect` that depends on `[idStatusKey, executionProcessesRaw, emitEntries]`:

  ```ts
  if (
    previousStatus === ExecutionProcessStatus.running &&
    currentStatus !== ExecutionProcessStatus.running &&
    displayedExecutionProcesses.current[process.id]
  ) {
    processesToReload.push(process);
  }
  ```

- [ ] **Step 2: Add the settled guard as a fourth condition**

  Replace:
  ```ts
  if (
    previousStatus === ExecutionProcessStatus.running &&
    currentStatus !== ExecutionProcessStatus.running &&
    displayedExecutionProcesses.current[process.id]
  ) {
    processesToReload.push(process);
  }
  ```
  With:
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

  **Why this is safe:** If the live stream completed normally (`onFinished` fired → process is in `settledStreamProcessIdsRef`), Effect C skips the re-fetch. If the live stream failed or timed out (`onFinished` never fired → process is NOT in `settledStreamProcessIdsRef`), Effect C still re-fetches from the historic endpoint. This preserves Effect C as an error-recovery mechanism while eliminating the double-emit on the happy path.

- [ ] **Step 3: Run TypeScript check — expect no errors**

  ```bash
  pnpm run web-core:check
  ```

  Expected output: exits 0.

- [ ] **Step 4: Commit**

  ```bash
  git add packages/web-core/src/features/workspace-chat/model/hooks/useConversationHistory.ts
  git commit -m "fix(chat): skip historic re-fetch in Effect C when stream already settled cleanly"
  ```

---

## Task 4: Clear `settledStreamProcessIdsRef` on scope reset

**File:** `packages/web-core/src/features/workspace-chat/model/hooks/useConversationHistory.ts`

- [ ] **Step 1: Locate the scope-reset `useEffect`**

  Find the block around line 382:

  ```ts
  useEffect(() => {
    displayedExecutionProcesses.current = {};
    loadedInitialEntries.current = false;
    emittedEmptyInitialRef.current = false;
    streamingProcessIdsRef.current.clear();
    previousStatusMapRef.current.clear();
    emitEntries(displayedExecutionProcesses.current, 'initial', true);
  }, [scopeKey, emitEntries]);
  ```

- [ ] **Step 2: Add the clear immediately after `streamingProcessIdsRef.current.clear()`**

  Replace:
  ```ts
  useEffect(() => {
    displayedExecutionProcesses.current = {};
    loadedInitialEntries.current = false;
    emittedEmptyInitialRef.current = false;
    streamingProcessIdsRef.current.clear();
    previousStatusMapRef.current.clear();
    emitEntries(displayedExecutionProcesses.current, 'initial', true);
  }, [scopeKey, emitEntries]);
  ```
  With:
  ```ts
  useEffect(() => {
    displayedExecutionProcesses.current = {};
    loadedInitialEntries.current = false;
    emittedEmptyInitialRef.current = false;
    streamingProcessIdsRef.current.clear();
    settledStreamProcessIdsRef.current.clear();
    previousStatusMapRef.current.clear();
    emitEntries(displayedExecutionProcesses.current, 'initial', true);
  }, [scopeKey, emitEntries]);
  ```

  **Why needed:** `scopeKey` changes when the user navigates to a different task/workspace. Without this clear, settled IDs from a previous scope would bleed into the new scope, preventing Effect C from correctly re-fetching processes that share the same ID in a fresh context.

- [ ] **Step 3: Run full TypeScript check + lint**

  ```bash
  pnpm run web-core:check && pnpm run lint
  ```

  Expected output: both exit 0, no errors or warnings on modified files.

- [ ] **Step 4: Run formatter to ensure consistent code style**

  ```bash
  pnpm run format
  ```

- [ ] **Step 5: Commit**

  ```bash
  git add packages/web-core/src/features/workspace-chat/model/hooks/useConversationHistory.ts
  git commit -m "fix(chat): clear settledStreamProcessIdsRef on scope reset"
  ```

---

## Task 5: Manual E2E validation

The hook cannot be unit-tested without React Testing Library + vitest setup (neither is configured in this package). Validate the fix manually against these specific scenarios.

**Prerequisites:** run `pnpm run dev` and open the local web app.

### Scenario A — Normal completion: no duplication (happy path fix)

- [ ] Start a new task and send a message to the coding agent
- [ ] Watch the chat log while the agent streams its response
- [ ] Wait for the agent to finish (status changes from running → completed)
- [ ] **Pass:** each assistant message, tool-use block, and thinking group appears exactly once in the final chat log
- [ ] **Fail (regression):** any content block appears twice, or content flickers/disappears then reappears

### Scenario B — Error recovery: Effect C still fires for failed streams

To verify Effect C still works as a fallback:

- [ ] Start a task with a coding agent
- [ ] While the agent is running, use DevTools → Network tab → find the `normalized-logs/ws` WebSocket connection → close it manually (DevTools: right-click → "Close")
- [ ] The backoff retry logic in `loadRunningAndEmitWithBackoff` will retry; if the process has already finished by the time you close it, the `onFinished` callback will NOT have fired (it fires only on the stream's `Finished` message)
- [ ] Confirm the chat log still shows the final entries (Effect C detects the status change and re-fetches because `settledStreamProcessIdsRef` does NOT contain the ID)
- [ ] **Pass:** chat log shows complete entries after the forced disconnect
- [ ] **Fail (regression):** chat log shows empty or incomplete entries after disconnect

### Scenario C — Multiple agents in sequence: no cross-contamination

- [ ] Send a message → wait for agent to complete
- [ ] Send a follow-up message → wait for second agent to complete
- [ ] **Pass:** each agent turn shows its entries exactly once; no entries from turn 1 appear in turn 2's section
- [ ] **Fail:** entries bleed between turns, or turn 2's entries duplicate turn 1's

### Scenario D — Scope change: settled IDs cleared correctly

- [ ] Complete a task (agent finishes, entries stable)
- [ ] Navigate to a different task
- [ ] Navigate back to the first task
- [ ] **Pass:** entries load correctly from historic endpoint; no duplication
- [ ] **Fail:** entries missing or the settled-ID guard incorrectly blocks Effect C from loading the returning task's history

---

## Summary of changes

All 4 line additions are in a single file:

```javascript
packages/web-core/src/features/workspace-chat/model/hooks/useConversationHistory.ts

Line ~42:  + const settledStreamProcessIdsRef = useRef<Set<string>>(new Set());
Line ~231: + settledStreamProcessIdsRef.current.add(executionProcess.id);
Line ~386: + settledStreamProcessIdsRef.current.clear();
Line ~487: + && !settledStreamProcessIdsRef.current.has(process.id)
```

Total: **+4 lines, 0 deletions, 0 other files changed.**
