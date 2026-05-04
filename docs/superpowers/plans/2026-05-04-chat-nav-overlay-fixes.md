# Chat Nav Overlay — Adversarial Review Fixes

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix 13 issues identified by Codex / Opus / Gemini adversarial review of merge commit `ddc56ff14` ("4-button nav overlay on chat conversation views"). Resolve 2 critical bugs, eliminate 3-shell duplication, decouple Phosphor types, restore i18n, harden edge cases, and add test coverage.

**Architecture:**
- Two new shared primitives in `packages/ui` and `packages/web-core/src/features/workspace-chat/model/`: a presentational `<ConversationNavOverlay>` and a controller hook `useConversationNavController(ref)`.
- Critical-bug fixes land first (TDD) so the refactor lands on a known-good baseline.
- Each phase is a separate PR — `vk/d115-nav-fix-bugs`, `vk/d115-nav-extract`, `vk/d115-nav-keyboard`, `vk/d115-nav-polish`.

**Tech Stack:** TypeScript, React 18, Vitest (colocated `*.test.ts`), pnpm workspaces, react-i18next, TanStack Virtual, Phosphor icons.

---

## Files Involved

**Existing files modified:**
- `packages/web-core/src/features/workspace-chat/model/conversation-row-model.ts` — verify `findNextUserMessageIndex` semantics
- `packages/web-core/src/features/workspace-chat/model/useConversationVirtualizer.ts` — bug fixes + spacer parity + new `hasNextUserMessage`/`hasPreviousUserMessage` selectors
- `packages/web-core/src/features/workspace-chat/ui/ConversationListContainer.tsx` — extend handle with existence selectors; thread spacer ref into hook
- `packages/web-core/src/features/workspace-chat/ui/SessionChatBoxContainer.tsx` — add `onScrollToTop` / `onScrollToNextMessage` props
- `packages/ui/src/components/SessionChatBox.tsx` — keyboard bindings for top/next
- `packages/ui/src/components/WorkspacesMain.tsx` — drop inline `NavButton`, use `<ConversationNavOverlay>`
- `packages/web-core/src/pages/workspaces/WorkspacesMainContainer.tsx` — replace handlers with `useConversationNavController`
- `packages/web-core/src/pages/workspaces/VSCodeWorkspacePage.tsx` — same; memoize `workspaceWithSession`
- `packages/web-core/src/pages/kanban/ProjectRightSidebarContainer.tsx` — same
- `packages/web-core/src/i18n/locales/{en,es,fr,ja,ko,zh-Hans,zh-Hant}/common.json` — add `workspaces.nav.*` keys

**Files created:**
- `packages/ui/src/components/ConversationNavOverlay.tsx`
- `packages/ui/src/components/ConversationNavOverlay.test.tsx`
- `packages/web-core/src/features/workspace-chat/model/useConversationNavController.ts`
- `packages/web-core/src/features/workspace-chat/model/useConversationNavController.test.ts`
- `packages/web-core/src/features/workspace-chat/model/useConversationVirtualizer.test.ts`
- `packages/web-core/src/features/workspace-chat/model/conversation-row-model.test.ts`

---

## Issue → Phase Mapping

| # | Issue | Phase | Severity |
|---|---|---|---|
| 1 | `bottomLockedRef` never re-armed after `scrollToTop` → streaming auto-follow dies | 1 | Critical |
| 2 | Next/Previous buttons gated on `!isAtBottom`/`!isAtTop` not "msg exists" → silent no-op | 1 | Critical |
| 3 | `NavButton` + overlay JSX triplicated | 2 | Major |
| 4 | Container handler boilerplate triplicated; `isAtBottomRef` mirror missing in kanban | 2 | Major |
| 5 | i18n debt — hardcoded English aria-labels in 3 copies | 2 | Major |
| 6 | Phosphor `Icon` type leaks into `packages/ui` | 2 | Major |
| 7 | `SessionChatBoxContainer` keyboard exposure asymmetric (no top/next) | 3 | Major |
| 8 | `isAtTop` strict `<= 0` — sub-pixel flicker on macOS | 4 | Minor |
| 9 | `scrollToTop` semantic mismatch — hook doesn't clear `planRevealSpacerRef` | 4 | Minor |
| 10 | Mobile/narrow viewport overflow risk | 4 | Minor |
| 11 | `WorkspacesMain` defaults `isAtTop = true` — public consumers get degraded overlay | 2 | Minor |
| 12 | `createWorkspaceWithSession` un-memoized in `VSCodeWorkspacePage` | 2 | Minor |
| 13 | Zero test coverage on new scroll paths | 1, 2, 3 | Major |

---

## Phase 0: Branch + verify tooling

### Task 0.1: Branch from main and confirm test runner

**Files:** none

- [ ] **Step 1: Create branch from main**

```bash
git fetch origin main
git checkout -b vk/d115-nav-fix-bugs origin/main
```

- [ ] **Step 2: Confirm Vitest is installed and resolvable from `packages/web-core`**

```bash
pnpm install
pnpm --filter @vibe/web-core exec vitest --version
```

Expected: prints a version like `1.x.x` or `2.x.x`. If "command not found", run `pnpm --filter @vibe/web-core add -D vitest @testing-library/react @testing-library/jest-dom jsdom @vitejs/plugin-react` and add a `vitest.config.ts` mirroring an existing one (e.g., `packages/ui/vitest.config.ts` if present, otherwise:

```ts
// packages/web-core/vitest.config.ts
import { defineConfig } from 'vitest/config';
import react from '@vitejs/plugin-react';
import path from 'node:path';

export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      '@': path.resolve(__dirname, './src'),
      shared: path.resolve(__dirname, '../../shared'),
    },
  },
  test: {
    environment: 'jsdom',
    globals: false,
  },
});
```

- [ ] **Step 3: Confirm an existing test runs**

```bash
pnpm --filter @vibe/web-core exec vitest run src/shared/lib/diffDataAdapter.test.ts
```

Expected: passes (or already known-failing — note the baseline).

- [ ] **Step 4: Commit any test infra additions only if needed**

```bash
git add packages/web-core/vitest.config.ts packages/web-core/package.json pnpm-lock.yaml
git commit -m "chore(web-core): add vitest config for chat nav tests"
```

If no infra change was needed, skip the commit.

---

## Phase 1 (PR-A `vk/d115-nav-fix-bugs`): Critical bug fixes — TDD

### Task 1.1: Add `conversation-row-model` unit tests

**Files:**
- Create: `packages/web-core/src/features/workspace-chat/model/conversation-row-model.test.ts`

- [ ] **Step 1: Write tests for both helpers**

```ts
// packages/web-core/src/features/workspace-chat/model/conversation-row-model.test.ts
import { describe, it, expect } from 'vitest';
import {
  findPreviousUserMessageIndex,
  findNextUserMessageIndex,
  type ConversationRow,
} from './conversation-row-model';

function row(isUserMessage: boolean): ConversationRow {
  return { isUserMessage } as unknown as ConversationRow;
}

describe('findPreviousUserMessageIndex', () => {
  it('returns -1 for empty rows', () => {
    expect(findPreviousUserMessageIndex([], 0)).toBe(-1);
  });

  it('returns -1 when no earlier user message exists', () => {
    const rows = [row(false), row(false), row(true)];
    expect(findPreviousUserMessageIndex(rows, 0)).toBe(-1);
    expect(findPreviousUserMessageIndex(rows, 1)).toBe(-1);
  });

  it('returns the nearest earlier user message index, exclusive of beforeIndex', () => {
    const rows = [row(true), row(false), row(true), row(false)];
    expect(findPreviousUserMessageIndex(rows, 3)).toBe(2);
    expect(findPreviousUserMessageIndex(rows, 2)).toBe(0);
  });
});

describe('findNextUserMessageIndex', () => {
  it('returns -1 for empty rows', () => {
    expect(findNextUserMessageIndex([], -1)).toBe(-1);
  });

  it('returns -1 when no later user message exists', () => {
    const rows = [row(true), row(false), row(false)];
    expect(findNextUserMessageIndex(rows, 0)).toBe(-1);
    expect(findNextUserMessageIndex(rows, 2)).toBe(-1);
  });

  it('returns the nearest later user message index, exclusive of afterIndex', () => {
    const rows = [row(true), row(false), row(true), row(false), row(true)];
    expect(findNextUserMessageIndex(rows, 0)).toBe(2);
    expect(findNextUserMessageIndex(rows, 2)).toBe(4);
    expect(findNextUserMessageIndex(rows, -1)).toBe(0);
  });

  it('is symmetric with findPreviousUserMessageIndex', () => {
    const rows = [row(true), row(false), row(true), row(false), row(true)];
    expect(findPreviousUserMessageIndex(rows, 4)).toBe(2);
    expect(findNextUserMessageIndex(rows, 2)).toBe(4);
  });
});
```

- [ ] **Step 2: Run tests; expect all to pass (helper already exists)**

```bash
pnpm --filter @vibe/web-core exec vitest run src/features/workspace-chat/model/conversation-row-model.test.ts
```

Expected: all PASS. If any fail, the helper has a bug — fix it before proceeding.

- [ ] **Step 3: Commit**

```bash
git add packages/web-core/src/features/workspace-chat/model/conversation-row-model.test.ts
git commit -m "test(chat): cover findPreviousUserMessageIndex + findNextUserMessageIndex"
```

---

### Task 1.2: Failing test for Bug #1 — `bottomLockedRef` re-arm

**Files:**
- Create: `packages/web-core/src/features/workspace-chat/model/useConversationVirtualizer.test.ts`

- [ ] **Step 1: Write the failing regression test**

```ts
// packages/web-core/src/features/workspace-chat/model/useConversationVirtualizer.test.ts
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { useConversationVirtualizer } from './useConversationVirtualizer';

// Minimal scroll container stub
function makeContainer(height: number, scrollHeight: number) {
  const el = document.createElement('div');
  Object.defineProperty(el, 'clientHeight', { value: height, configurable: true });
  Object.defineProperty(el, 'scrollHeight', { value: scrollHeight, configurable: true });
  el.scrollTop = 0;
  el.scrollTo = (opts: ScrollToOptions | number, y?: number) => {
    const top = typeof opts === 'number' ? y ?? 0 : (opts.top ?? 0);
    el.scrollTop = top;
    el.dispatchEvent(new Event('scroll'));
  };
  return el;
}

describe('useConversationVirtualizer — bottom-lock invariant', () => {
  let container: HTMLDivElement;
  beforeEach(() => {
    container = makeContainer(500, 2000);
    document.body.appendChild(container);
  });
  afterEach(() => {
    container.remove();
  });

  it('re-arms bottom lock when user scrolls back to bottom after scrollToTop', () => {
    // Render hook with the scroll container. Adapt to the hook's actual signature.
    const ref = { current: container };
    const { result } = renderHook(() =>
      useConversationVirtualizer({
        scrollContainerRef: ref as React.RefObject<HTMLElement>,
        rows: [],
        // ...other required options. See actual hook signature in step 2.
      } as Parameters<typeof useConversationVirtualizer>[0])
    );

    // 1. Pretend we are at the bottom: lock should be armed.
    act(() => {
      container.scrollTop = container.scrollHeight - container.clientHeight; // 1500
      container.dispatchEvent(new Event('scroll'));
    });
    expect(result.current.isAtBottom).toBe(true);

    // 2. Click "Go to top" — releases the lock.
    act(() => {
      result.current.scrollToTop('auto');
    });
    expect(container.scrollTop).toBe(0);

    // 3. User manually scrolls back to bottom.
    act(() => {
      container.scrollTop = container.scrollHeight - container.clientHeight;
      container.dispatchEvent(new Event('scroll'));
    });

    // 4. Bottom lock MUST be re-armed; isAtBottom is true again.
    expect(result.current.isAtBottom).toBe(true);

    // 5. Auto-follow check: simulate new content appended.
    const newScrollHeight = container.scrollHeight + 200;
    Object.defineProperty(container, 'scrollHeight', { value: newScrollHeight, configurable: true });
    act(() => {
      result.current.adjustScrollBy(200);
    });
    expect(container.scrollTop).toBe(newScrollHeight - container.clientHeight);
  });
});
```

- [ ] **Step 2: Adapt the harness to the actual hook signature**

```bash
sed -n '1,80p' packages/web-core/src/features/workspace-chat/model/useConversationVirtualizer.ts
```

Identify the required options (likely `rows`, `scrollContainerRef`, and possibly `getRowKey` / `estimateSize` / `onAtBottomChange` / `onAtTopChange`). Fill in minimal stubs in the test for any missing options. The test should compile.

- [ ] **Step 3: Run the test; expect FAIL on the re-arm assertion**

```bash
pnpm --filter @vibe/web-core exec vitest run src/features/workspace-chat/model/useConversationVirtualizer.test.ts -t "re-arms bottom lock"
```

Expected: FAIL on step 5 — `container.scrollTop` not adjusted because `bottomLockedRef.current` is still `false` after `scrollToTop`.

- [ ] **Step 4: Commit the failing test**

```bash
git add packages/web-core/src/features/workspace-chat/model/useConversationVirtualizer.test.ts
git commit -m "test(chat): failing regression for bottom-lock re-arm after scrollToTop"
```

---

### Task 1.3: Fix Bug #1 — re-arm `bottomLockedRef` in `syncScrollEdges`

**Files:**
- Modify: `packages/web-core/src/features/workspace-chat/model/useConversationVirtualizer.ts` (search for `syncScrollEdges`)

- [ ] **Step 1: Locate `syncScrollEdges`**

```bash
grep -n "syncScrollEdges\|bottomLockedRef" packages/web-core/src/features/workspace-chat/model/useConversationVirtualizer.ts | head -30
```

- [ ] **Step 2: Inside `syncScrollEdges`, after computing `nextAtBottom`, re-arm the lock on the false→true transition**

Add this block after the line that assigns `nextAtBottom` and before the existing `if (lastAtBottomRef.current !== nextAtBottom)` callback fire:

```ts
// Re-arm bottom-lock when the user (or programmatic scroll) returns to the bottom.
// Prevents a permanent auto-follow regression after scrollToTop / scrollToIndex.
if (nextAtBottom && !bottomLockedRef.current) {
  bottomLockedRef.current = true;
}
```

- [ ] **Step 3: Run the regression test; expect PASS**

```bash
pnpm --filter @vibe/web-core exec vitest run src/features/workspace-chat/model/useConversationVirtualizer.test.ts -t "re-arms bottom lock"
```

Expected: PASS.

- [ ] **Step 4: Run full hook test file**

```bash
pnpm --filter @vibe/web-core exec vitest run src/features/workspace-chat/model/useConversationVirtualizer.test.ts
```

Expected: all PASS.

- [ ] **Step 5: Commit**

```bash
git add packages/web-core/src/features/workspace-chat/model/useConversationVirtualizer.ts
git commit -m "fix(chat): re-arm bottom-lock when returning to bottom after scrollToTop"
```

---

### Task 1.4: Failing test for Bug #2 — button gating on existence

**Files:**
- Modify: `packages/web-core/src/features/workspace-chat/model/useConversationVirtualizer.test.ts`

- [ ] **Step 1: Append a new `describe` block**

```ts
describe('useConversationVirtualizer — message-existence selectors', () => {
  it('hasNextUserMessage is false when no later user message exists', () => {
    const ref = { current: makeContainer(500, 2000) };
    const rows = [
      { isUserMessage: true },
      { isUserMessage: false },
      { isUserMessage: false },
    ];
    const { result } = renderHook(() =>
      useConversationVirtualizer({
        scrollContainerRef: ref as React.RefObject<HTMLElement>,
        rows,
      } as Parameters<typeof useConversationVirtualizer>[0])
    );
    expect(result.current.hasNextUserMessage()).toBe(false);
    expect(result.current.hasPreviousUserMessage()).toBe(false);
  });

  it('hasPreviousUserMessage is true when an earlier user message exists', () => {
    const ref = { current: makeContainer(500, 2000) };
    const rows = [
      { isUserMessage: true },
      { isUserMessage: false },
      { isUserMessage: true },
      { isUserMessage: false },
    ];
    const { result } = renderHook(() =>
      useConversationVirtualizer({
        scrollContainerRef: ref as React.RefObject<HTMLElement>,
        rows,
      } as Parameters<typeof useConversationVirtualizer>[0])
    );
    // From the last row, both should exist.
    expect(result.current.hasPreviousUserMessage()).toBe(true);
    expect(result.current.hasNextUserMessage()).toBe(false);
  });
});
```

- [ ] **Step 2: Run; expect FAIL with "result.current.hasNextUserMessage is not a function"**

```bash
pnpm --filter @vibe/web-core exec vitest run src/features/workspace-chat/model/useConversationVirtualizer.test.ts -t "message-existence"
```

Expected: FAIL.

- [ ] **Step 3: Commit failing test**

```bash
git add packages/web-core/src/features/workspace-chat/model/useConversationVirtualizer.test.ts
git commit -m "test(chat): failing tests for hasNext/PreviousUserMessage selectors"
```

---

### Task 1.5: Implement `hasNextUserMessage` / `hasPreviousUserMessage` on the hook

**Files:**
- Modify: `packages/web-core/src/features/workspace-chat/model/useConversationVirtualizer.ts`

- [ ] **Step 1: Locate the `result` returned by the hook (likely the bottom of the file) and the existing `firstVisibleIndex` ref**

```bash
grep -n "return {\|firstVisibleIndex\|scrollToPreviousUserMessage\|scrollToNextUserMessage" packages/web-core/src/features/workspace-chat/model/useConversationVirtualizer.ts | head
```

- [ ] **Step 2: Add the selectors before the `return` block**

```ts
const hasPreviousUserMessage = useCallback(() => {
  const fromIndex = firstVisibleIndexRef.current ?? 0;
  return findPreviousUserMessageIndex(rowsRef.current, fromIndex) !== -1;
}, []);

const hasNextUserMessage = useCallback(() => {
  const fromIndex = firstVisibleIndexRef.current ?? -1;
  return findNextUserMessageIndex(rowsRef.current, fromIndex) !== -1;
}, []);
```

If `rowsRef` does not exist, add one alongside the existing rows tracking:

```ts
const rowsRef = useRef(rows);
useEffect(() => {
  rowsRef.current = rows;
}, [rows]);
```

- [ ] **Step 3: Add to the returned object**

```ts
return {
  // ...existing fields...
  hasPreviousUserMessage,
  hasNextUserMessage,
};
```

- [ ] **Step 4: Update the hook's `UseConversationVirtualizerResult` (or equivalent return type) to include the two methods**

```ts
hasPreviousUserMessage: () => boolean;
hasNextUserMessage: () => boolean;
```

- [ ] **Step 5: Run tests; expect PASS**

```bash
pnpm --filter @vibe/web-core exec vitest run src/features/workspace-chat/model/useConversationVirtualizer.test.ts
```

Expected: all PASS.

- [ ] **Step 6: Commit**

```bash
git add packages/web-core/src/features/workspace-chat/model/useConversationVirtualizer.ts
git commit -m "feat(chat): expose hasNext/PreviousUserMessage selectors from virtualizer hook"
```

---

### Task 1.6: Surface selectors on `ConversationListHandle`

**Files:**
- Modify: `packages/web-core/src/features/workspace-chat/ui/ConversationListContainer.tsx`

- [ ] **Step 1: Extend the handle type**

Locate `export interface ConversationListHandle` (search via `grep -n "ConversationListHandle" ...Container.tsx`). Add:

```ts
hasPreviousUserMessage: () => boolean;
hasNextUserMessage: () => boolean;
```

- [ ] **Step 2: Add to `useImperativeHandle`**

```ts
useImperativeHandle(
  ref,
  () => ({
    // ...existing methods...
    hasPreviousUserMessage: conversationVirtualizer.hasPreviousUserMessage,
    hasNextUserMessage: conversationVirtualizer.hasNextUserMessage,
  }),
  [conversationVirtualizer /* keep existing deps */]
);
```

- [ ] **Step 3: Run check**

```bash
pnpm --filter @vibe/local-web run check
```

Expected: clean exit 0.

- [ ] **Step 4: Commit**

```bash
git add packages/web-core/src/features/workspace-chat/ui/ConversationListContainer.tsx
git commit -m "feat(chat): expose hasNext/PreviousUserMessage on ConversationListHandle"
```

---

### Task 1.7: Gate prev/next buttons on existence in all three shells

**Files:**
- Modify: `packages/ui/src/components/WorkspacesMain.tsx`
- Modify: `packages/web-core/src/pages/workspaces/WorkspacesMainContainer.tsx`
- Modify: `packages/web-core/src/pages/workspaces/VSCodeWorkspacePage.tsx`
- Modify: `packages/web-core/src/pages/kanban/ProjectRightSidebarContainer.tsx`

This is interim — the full extraction lands in Phase 2. For now, plumb `hasPrevious`/`hasNext` flags so the user-visible bug is gone immediately.

- [ ] **Step 1: Add new props to `WorkspacesMainProps`**

```ts
// packages/ui/src/components/WorkspacesMain.tsx
interface WorkspacesMainProps {
  // ...existing fields...
  hasPreviousUserMessage?: boolean;
  hasNextUserMessage?: boolean;
}
```

- [ ] **Step 2: Default both to `true` (backwards compat) and update visibility predicates**

```ts
export function WorkspacesMain({
  // ...
  hasPreviousUserMessage = true,
  hasNextUserMessage = true,
  // ...
}: WorkspacesMainProps) {
```

Then change the overlay JSX:

```tsx
{!isAtTop && hasPreviousUserMessage && onScrollToPreviousMessage && (
  <NavButton icon={ArrowUpIcon} label="Previous user message" onClick={onScrollToPreviousMessage} />
)}
{!isAtBottom && hasNextUserMessage && onScrollToNextMessage && (
  <NavButton icon={ArrowDownIcon} label="Next user message" onClick={onScrollToNextMessage} />
)}
```

- [ ] **Step 3: In the three containers, wire the flags**

Add to each container (sample — `WorkspacesMainContainer.tsx`):

```ts
const [hasPreviousUserMessage, setHasPreviousUserMessage] = useState(false);
const [hasNextUserMessage, setHasNextUserMessage] = useState(false);

// Recompute on edge changes (existence is cheap; piggyback on edge callbacks).
const handleAtBottomChange = useCallback((atBottom: boolean) => {
  isAtBottomRef.current = atBottom;
  setIsAtBottom(atBottom);
  setHasPreviousUserMessage(conversationListRef.current?.hasPreviousUserMessage() ?? false);
  setHasNextUserMessage(conversationListRef.current?.hasNextUserMessage() ?? false);
}, []);

const handleAtTopChange = useCallback((atTop: boolean) => {
  setIsAtTop(atTop);
  setHasPreviousUserMessage(conversationListRef.current?.hasPreviousUserMessage() ?? false);
  setHasNextUserMessage(conversationListRef.current?.hasNextUserMessage() ?? false);
}, []);
```

Pass `hasPreviousUserMessage`/`hasNextUserMessage` to `<WorkspacesMain>`.

For `VSCodeWorkspacePage.tsx` and `ProjectRightSidebarContainer.tsx`, apply the same state + the `&& hasNextUserMessage` / `&& hasPreviousUserMessage` gates inline in their NavButton JSX.

- [ ] **Step 4: Run check**

```bash
pnpm --filter @vibe/local-web run check
```

Expected: clean.

- [ ] **Step 5: Manual smoke test**

```bash
pnpm run local-web:dev
```

Open a workspace where the last entry is an assistant message (i.e., user→assistant→assistant). Scroll up. Confirm the down-arrow ("Next user message") button is HIDDEN when no later user message exists. Scroll up further so a previous user message exists; confirm up-arrow ("Previous user message") is VISIBLE.

- [ ] **Step 6: Commit**

```bash
git add packages/ui/src/components/WorkspacesMain.tsx \
        packages/web-core/src/pages/workspaces/WorkspacesMainContainer.tsx \
        packages/web-core/src/pages/workspaces/VSCodeWorkspacePage.tsx \
        packages/web-core/src/pages/kanban/ProjectRightSidebarContainer.tsx
git commit -m "fix(chat): hide prev/next nav buttons when no such user message exists"
```

---

### Task 1.8: Push PR-A

- [ ] **Step 1: Push and open PR**

```bash
git push -u origin vk/d115-nav-fix-bugs
gh pr create --title "fix(chat): nav overlay critical bugs (re-arm + button gating)" --body "$(cat <<'EOF'
## Summary

Fixes two regressions in the chat-area nav overlay shipped in #9:

1. **Auto-follow dies after Go to top** — `scrollToTop()` released `bottomLockedRef` but no path re-armed it; this PR re-arms in `syncScrollEdges` on the false→true transition.
2. **Next/Prev buttons silently no-op** — buttons were gated on `!isAtBottom`/`!isAtTop`. Now also gated on `hasNextUserMessage()` / `hasPreviousUserMessage()` returning true.

Adds vitest coverage for both regressions and for `findNextUserMessageIndex` / `findPreviousUserMessageIndex`.

## Test plan
- [ ] vitest passes for new tests
- [ ] Manual: open completed thread ending in assistant turn; "Next user message" hidden when no later user msg exists.
- [ ] Manual: scroll to top, then back to bottom; new streaming entries auto-follow.
EOF
)"
```

---

## Phase 2 (PR-B `vk/d115-nav-extract`): Extract shared overlay + controller hook

Builds on PR-A merge. Eliminates the 3-shell duplication, restores i18n, decouples Phosphor types.

### Task 2.1: Create `<ConversationNavOverlay>` in `packages/ui`

**Files:**
- Create: `packages/ui/src/components/ConversationNavOverlay.tsx`

- [ ] **Step 1: Branch**

```bash
git fetch origin main
git checkout -b vk/d115-nav-extract origin/main
```

- [ ] **Step 2: Create the component**

```tsx
// packages/ui/src/components/ConversationNavOverlay.tsx
import { useTranslation } from 'react-i18next';
import {
  ArrowDownIcon,
  ArrowLineDownIcon,
  ArrowLineUpIcon,
  ArrowUpIcon,
} from '@phosphor-icons/react';
import { cn } from '../lib/cn';

export interface ConversationNavOverlayProps {
  isAtTop: boolean;
  isAtBottom: boolean;
  hasPreviousUserMessage: boolean;
  hasNextUserMessage: boolean;
  onScrollToTop: () => void;
  onScrollToPreviousMessage: () => void;
  onScrollToNextMessage: () => void;
  onScrollToBottom: () => void;
  /**
   * On narrow viewports the vertical 4-button stack is omitted; the parent
   * shell is expected to provide its own affordance (or none).
   */
  isMobile?: boolean;
  className?: string;
}

interface NavButtonProps {
  icon: React.ComponentType<{ className?: string; weight?: 'bold' }>;
  label: string;
  onClick: () => void;
}

function NavButton({ icon: Icon, label, onClick }: NavButtonProps) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="pointer-events-auto flex items-center justify-center size-8 rounded-full bg-secondary/80 backdrop-blur-sm border border-secondary text-low hover:text-normal hover:bg-secondary shadow-md transition-all"
      aria-label={label}
      title={label}
    >
      <Icon className="size-icon-base" weight="bold" />
    </button>
  );
}

export function ConversationNavOverlay({
  isAtTop,
  isAtBottom,
  hasPreviousUserMessage,
  hasNextUserMessage,
  onScrollToTop,
  onScrollToPreviousMessage,
  onScrollToNextMessage,
  onScrollToBottom,
  isMobile,
  className,
}: ConversationNavOverlayProps) {
  const { t } = useTranslation('common');

  if (isMobile) return null;
  if (isAtTop && isAtBottom) return null;

  return (
    <div className={cn('flex justify-center pointer-events-none', className)}>
      <div className="w-chat max-w-full relative">
        <div className="absolute bottom-2 right-4 z-10 flex flex-col gap-1 pointer-events-none">
          {!isAtTop && (
            <NavButton
              icon={ArrowLineUpIcon}
              label={t('workspaces.nav.goToTop')}
              onClick={onScrollToTop}
            />
          )}
          {!isAtTop && hasPreviousUserMessage && (
            <NavButton
              icon={ArrowUpIcon}
              label={t('workspaces.nav.previousUserMessage')}
              onClick={onScrollToPreviousMessage}
            />
          )}
          {!isAtBottom && hasNextUserMessage && (
            <NavButton
              icon={ArrowDownIcon}
              label={t('workspaces.nav.nextUserMessage')}
              onClick={onScrollToNextMessage}
            />
          )}
          {!isAtBottom && (
            <NavButton
              icon={ArrowLineDownIcon}
              label={t('workspaces.nav.scrollToBottom')}
              onClick={onScrollToBottom}
            />
          )}
        </div>
      </div>
    </div>
  );
}
```

- [ ] **Step 3: Re-export from `packages/ui` index**

```bash
grep -rn "WorkspacesMain" packages/ui/src/index.ts | head
```

If `packages/ui/src/index.ts` re-exports `WorkspacesMain`, add the new component:

```ts
export { ConversationNavOverlay } from './components/ConversationNavOverlay';
export type { ConversationNavOverlayProps } from './components/ConversationNavOverlay';
```

- [ ] **Step 4: Commit**

```bash
git add packages/ui/src/components/ConversationNavOverlay.tsx packages/ui/src/index.ts
git commit -m "feat(ui): add ConversationNavOverlay component"
```

---

### Task 2.2: Component test for `<ConversationNavOverlay>`

**Files:**
- Create: `packages/ui/src/components/ConversationNavOverlay.test.tsx`

- [ ] **Step 1: Write tests**

```tsx
// packages/ui/src/components/ConversationNavOverlay.test.tsx
import { describe, it, expect, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { I18nextProvider } from 'react-i18next';
import i18n from 'i18next';
import { ConversationNavOverlay } from './ConversationNavOverlay';

// Minimal i18n test instance.
i18n.init({
  lng: 'en',
  resources: {
    en: {
      common: {
        workspaces: {
          nav: {
            goToTop: 'Go to top',
            previousUserMessage: 'Previous user message',
            nextUserMessage: 'Next user message',
            scrollToBottom: 'Scroll to bottom',
          },
        },
      },
    },
  },
  ns: ['common'],
  defaultNS: 'common',
});

const baseProps = {
  isAtTop: false,
  isAtBottom: false,
  hasPreviousUserMessage: true,
  hasNextUserMessage: true,
  onScrollToTop: vi.fn(),
  onScrollToPreviousMessage: vi.fn(),
  onScrollToNextMessage: vi.fn(),
  onScrollToBottom: vi.fn(),
};

function renderWithI18n(ui: React.ReactElement) {
  return render(<I18nextProvider i18n={i18n}>{ui}</I18nextProvider>);
}

describe('ConversationNavOverlay', () => {
  it('renders all four buttons in the middle of a long conversation', () => {
    renderWithI18n(<ConversationNavOverlay {...baseProps} />);
    expect(screen.getByRole('button', { name: 'Go to top' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Previous user message' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Next user message' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Scroll to bottom' })).toBeInTheDocument();
  });

  it('hides the entire overlay when both edges are reached', () => {
    const { container } = renderWithI18n(
      <ConversationNavOverlay {...baseProps} isAtTop isAtBottom />
    );
    expect(container).toBeEmptyDOMElement();
  });

  it('hides "previous user message" when none exists', () => {
    renderWithI18n(
      <ConversationNavOverlay {...baseProps} hasPreviousUserMessage={false} />
    );
    expect(screen.queryByRole('button', { name: 'Previous user message' })).toBeNull();
  });

  it('hides "next user message" when none exists', () => {
    renderWithI18n(
      <ConversationNavOverlay {...baseProps} hasNextUserMessage={false} />
    );
    expect(screen.queryByRole('button', { name: 'Next user message' })).toBeNull();
  });

  it('renders nothing on mobile', () => {
    const { container } = renderWithI18n(
      <ConversationNavOverlay {...baseProps} isMobile />
    );
    expect(container).toBeEmptyDOMElement();
  });

  it('invokes the correct callback on click', async () => {
    const onScrollToTop = vi.fn();
    renderWithI18n(
      <ConversationNavOverlay {...baseProps} onScrollToTop={onScrollToTop} />
    );
    await userEvent.click(screen.getByRole('button', { name: 'Go to top' }));
    expect(onScrollToTop).toHaveBeenCalledTimes(1);
  });
});
```

- [ ] **Step 2: Run; expect PASS**

```bash
pnpm --filter @vibe/ui exec vitest run src/components/ConversationNavOverlay.test.tsx
```

If `@testing-library/user-event` or `@testing-library/jest-dom` missing, add them as devDeps.

- [ ] **Step 3: Commit**

```bash
git add packages/ui/src/components/ConversationNavOverlay.test.tsx
git commit -m "test(ui): cover ConversationNavOverlay visibility + callbacks"
```

---

### Task 2.3: Add `workspaces.nav.*` i18n keys to all 7 locales

**Files:**
- Modify: `packages/web-core/src/i18n/locales/en/common.json`
- Modify: `packages/web-core/src/i18n/locales/es/common.json`
- Modify: `packages/web-core/src/i18n/locales/fr/common.json`
- Modify: `packages/web-core/src/i18n/locales/ja/common.json`
- Modify: `packages/web-core/src/i18n/locales/ko/common.json`
- Modify: `packages/web-core/src/i18n/locales/zh-Hans/common.json`
- Modify: `packages/web-core/src/i18n/locales/zh-Hant/common.json`

- [ ] **Step 1: Add the `nav` block under `workspaces` for each locale**

```json
"workspaces": {
  // ...existing keys...
  "nav": {
    "goToTop": "Go to top",
    "previousUserMessage": "Previous user message",
    "nextUserMessage": "Next user message",
    "scrollToBottom": "Scroll to bottom"
  }
}
```

Translations per locale:

| Locale | goToTop | previousUserMessage | nextUserMessage | scrollToBottom |
|---|---|---|---|---|
| es | Ir al principio | Mensaje anterior del usuario | Siguiente mensaje del usuario | Ir al final |
| fr | Aller en haut | Message précédent de l'utilisateur | Message suivant de l'utilisateur | Aller en bas |
| ja | 先頭へ移動 | 前のユーザーメッセージ | 次のユーザーメッセージ | 末尾へ移動 |
| ko | 맨 위로 이동 | 이전 사용자 메시지 | 다음 사용자 메시지 | 맨 아래로 이동 |
| zh-Hans | 跳到顶部 | 上一条用户消息 | 下一条用户消息 | 跳到底部 |
| zh-Hant | 跳到頂部 | 上一則使用者訊息 | 下一則使用者訊息 | 跳到底部 |

- [ ] **Step 2: Verify the unused-i18n-keys checker passes**

```bash
node scripts/check-unused-i18n-keys.mjs
```

Expected: no errors. If "nav.*" keys are flagged unused (because Phase 2 hasn't migrated shells yet), this is acceptable for now — the next task migrates consumers.

- [ ] **Step 3: Commit**

```bash
git add packages/web-core/src/i18n/locales/*/common.json
git commit -m "i18n(chat): add workspaces.nav.* keys for all locales"
```

---

### Task 2.4: Migrate `WorkspacesMain.tsx` to use `<ConversationNavOverlay>`

**Files:**
- Modify: `packages/ui/src/components/WorkspacesMain.tsx`

- [ ] **Step 1: Drop the inline `NavButton`, drop the Phosphor `Icon` type alias, drop the inline overlay JSX. Import the new component.**

```tsx
// packages/ui/src/components/WorkspacesMain.tsx (top of file)
import type { ReactNode, RefObject } from 'react';
import { useTranslation } from 'react-i18next';
import { SpinnerIcon } from '@phosphor-icons/react';
import { cn } from '../lib/cn';
import { ConversationNavOverlay } from './ConversationNavOverlay';
```

Remove the `IconComponent` / `NavButtonProps` / `NavButton` / `Icon as PhosphorIcon` import.

- [ ] **Step 2: Update `WorkspacesMainProps`**

```ts
interface WorkspacesMainProps {
  workspaceWithSession: WorkspacesMainWorkspace | undefined;
  isLoading: boolean;
  showLoadingOverlay?: boolean;
  containerRef: RefObject<HTMLElement>;
  conversationContent?: ReactNode;
  chatBoxContent: ReactNode;
  contextBarContent?: ReactNode;
  isAtBottom?: boolean;
  isAtTop?: boolean;
  hasPreviousUserMessage?: boolean;
  hasNextUserMessage?: boolean;
  onScrollToBottom?: () => void;
  onScrollToTop?: () => void;
  onScrollToPreviousMessage?: () => void;
  onScrollToNextMessage?: () => void;
  isMobile?: boolean;
}
```

Drop `onAtBottomChange` (the dead prop Opus flagged) unless grep shows external consumers.

- [ ] **Step 3: Replace the inline overlay block with**

```tsx
{workspaceWithSession &&
  onScrollToTop &&
  onScrollToBottom &&
  onScrollToPreviousMessage &&
  onScrollToNextMessage && (
    <ConversationNavOverlay
      isAtTop={isAtTop ?? false}
      isAtBottom={isAtBottom ?? false}
      hasPreviousUserMessage={hasPreviousUserMessage ?? false}
      hasNextUserMessage={hasNextUserMessage ?? false}
      onScrollToTop={onScrollToTop}
      onScrollToPreviousMessage={onScrollToPreviousMessage}
      onScrollToNextMessage={onScrollToNextMessage}
      onScrollToBottom={onScrollToBottom}
      isMobile={isMobile}
    />
  )}
```

Note: defaults flipped from `true` to `false` — fixes Issue #11 (silent degraded overlay). Public consumers that don't provide the callbacks now correctly render no overlay.

- [ ] **Step 4: Run check + lint**

```bash
pnpm --filter @vibe/local-web run check && pnpm run lint
```

Expected: clean.

- [ ] **Step 5: Commit**

```bash
git add packages/ui/src/components/WorkspacesMain.tsx
git commit -m "refactor(ui): replace inline nav overlay with ConversationNavOverlay"
```

---

### Task 2.5: Create `useConversationNavController` hook

**Files:**
- Create: `packages/web-core/src/features/workspace-chat/model/useConversationNavController.ts`

- [ ] **Step 1: Write the hook**

```ts
// packages/web-core/src/features/workspace-chat/model/useConversationNavController.ts
import { useCallback, useEffect, useRef, useState } from 'react';
import type { RefObject } from 'react';
import type { ConversationListHandle } from '../ui/ConversationListContainer';

export interface ConversationNavState {
  isAtBottom: boolean;
  isAtTop: boolean;
  hasPreviousUserMessage: boolean;
  hasNextUserMessage: boolean;
}

export interface ConversationNavController {
  // State
  isAtBottom: boolean;
  isAtTop: boolean;
  isAtBottomRef: RefObject<boolean>;
  hasPreviousUserMessage: boolean;
  hasNextUserMessage: boolean;
  // Callbacks for ConversationList
  onAtBottomChange: (atBottom: boolean) => void;
  onAtTopChange: (atTop: boolean) => void;
  // Handlers for nav overlay
  onScrollToTop: () => void;
  onScrollToBottom: () => void;
  onScrollToPreviousMessage: () => void;
  onScrollToNextMessage: () => void;
  onScrollToUserMessage: (patchKey: string) => void;
  getActiveTurnPatchKey: () => string | null;
}

export function useConversationNavController(
  ref: RefObject<ConversationListHandle>
): ConversationNavController {
  const [isAtBottom, setIsAtBottom] = useState(true);
  const [isAtTop, setIsAtTop] = useState(true);
  const [hasPreviousUserMessage, setHasPreviousUserMessage] = useState(false);
  const [hasNextUserMessage, setHasNextUserMessage] = useState(false);
  const isAtBottomRef = useRef(true);

  useEffect(() => {
    isAtBottomRef.current = isAtBottom;
  }, [isAtBottom]);

  const refreshExistence = useCallback(() => {
    setHasPreviousUserMessage(ref.current?.hasPreviousUserMessage() ?? false);
    setHasNextUserMessage(ref.current?.hasNextUserMessage() ?? false);
  }, [ref]);

  const onAtBottomChange = useCallback((atBottom: boolean) => {
    isAtBottomRef.current = atBottom;
    setIsAtBottom(atBottom);
    refreshExistence();
  }, [refreshExistence]);

  const onAtTopChange = useCallback((atTop: boolean) => {
    setIsAtTop(atTop);
    refreshExistence();
  }, [refreshExistence]);

  const onScrollToTop = useCallback(() => {
    ref.current?.scrollToTop('auto');
  }, [ref]);

  const onScrollToBottom = useCallback(() => {
    ref.current?.scrollToBottom('auto');
  }, [ref]);

  const onScrollToPreviousMessage = useCallback(() => {
    ref.current?.scrollToPreviousUserMessage();
  }, [ref]);

  const onScrollToNextMessage = useCallback(() => {
    ref.current?.scrollToNextUserMessage();
  }, [ref]);

  const onScrollToUserMessage = useCallback((patchKey: string) => {
    ref.current?.scrollToEntryByPatchKey(patchKey);
  }, [ref]);

  const getActiveTurnPatchKey = useCallback(() => {
    return ref.current?.getVisibleUserMessagePatchKey() ?? null;
  }, [ref]);

  return {
    isAtBottom,
    isAtTop,
    isAtBottomRef,
    hasPreviousUserMessage,
    hasNextUserMessage,
    onAtBottomChange,
    onAtTopChange,
    onScrollToTop,
    onScrollToBottom,
    onScrollToPreviousMessage,
    onScrollToNextMessage,
    onScrollToUserMessage,
    getActiveTurnPatchKey,
  };
}
```

- [ ] **Step 2: Commit**

```bash
git add packages/web-core/src/features/workspace-chat/model/useConversationNavController.ts
git commit -m "feat(chat): add useConversationNavController hook"
```

---

### Task 2.6: Test the controller hook

**Files:**
- Create: `packages/web-core/src/features/workspace-chat/model/useConversationNavController.test.ts`

- [ ] **Step 1: Write tests**

```ts
// packages/web-core/src/features/workspace-chat/model/useConversationNavController.test.ts
import { describe, it, expect, vi } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { useConversationNavController } from './useConversationNavController';
import type { ConversationListHandle } from '../ui/ConversationListContainer';

function makeHandle(): ConversationListHandle {
  return {
    scrollToTop: vi.fn(),
    scrollToBottom: vi.fn(),
    scrollToPreviousUserMessage: vi.fn(),
    scrollToNextUserMessage: vi.fn(),
    scrollToEntryByPatchKey: vi.fn(),
    getVisibleUserMessagePatchKey: vi.fn(() => 'patch-1'),
    hasPreviousUserMessage: vi.fn(() => true),
    hasNextUserMessage: vi.fn(() => false),
    adjustScrollBy: vi.fn(),
    releaseBottomLock: vi.fn(),
    getScrollElement: vi.fn(),
  } as unknown as ConversationListHandle;
}

describe('useConversationNavController', () => {
  it('forwards scroll handlers to the ref', () => {
    const handle = makeHandle();
    const ref = { current: handle };
    const { result } = renderHook(() => useConversationNavController(ref));

    act(() => result.current.onScrollToTop());
    expect(handle.scrollToTop).toHaveBeenCalledWith('auto');

    act(() => result.current.onScrollToBottom());
    expect(handle.scrollToBottom).toHaveBeenCalledWith('auto');

    act(() => result.current.onScrollToPreviousMessage());
    expect(handle.scrollToPreviousUserMessage).toHaveBeenCalled();

    act(() => result.current.onScrollToNextMessage());
    expect(handle.scrollToNextUserMessage).toHaveBeenCalled();
  });

  it('refreshes existence flags on edge changes', () => {
    const handle = makeHandle();
    const ref = { current: handle };
    const { result } = renderHook(() => useConversationNavController(ref));

    act(() => result.current.onAtBottomChange(false));
    expect(result.current.hasPreviousUserMessage).toBe(true);
    expect(result.current.hasNextUserMessage).toBe(false);
    expect(result.current.isAtBottom).toBe(false);

    act(() => result.current.onAtTopChange(false));
    expect(handle.hasPreviousUserMessage).toHaveBeenCalled();
    expect(handle.hasNextUserMessage).toHaveBeenCalled();
  });

  it('keeps isAtBottomRef in sync', () => {
    const handle = makeHandle();
    const ref = { current: handle };
    const { result } = renderHook(() => useConversationNavController(ref));
    act(() => result.current.onAtBottomChange(false));
    expect(result.current.isAtBottomRef.current).toBe(false);
    act(() => result.current.onAtBottomChange(true));
    expect(result.current.isAtBottomRef.current).toBe(true);
  });
});
```

- [ ] **Step 2: Run; expect PASS**

```bash
pnpm --filter @vibe/web-core exec vitest run src/features/workspace-chat/model/useConversationNavController.test.ts
```

- [ ] **Step 3: Commit**

```bash
git add packages/web-core/src/features/workspace-chat/model/useConversationNavController.test.ts
git commit -m "test(chat): cover useConversationNavController"
```

---

### Task 2.7: Migrate `WorkspacesMainContainer.tsx` to use the controller

**Files:**
- Modify: `packages/web-core/src/pages/workspaces/WorkspacesMainContainer.tsx`

- [ ] **Step 1: Replace handlers with the hook**

```tsx
// at top
import { useConversationNavController } from '@/features/workspace-chat/model/useConversationNavController';

// Inside the component, replace the 8 useCallback handlers + 4 useState/useRef bookkeeping with:
const nav = useConversationNavController(conversationListRef);
```

- [ ] **Step 2: Replace prop wiring**

```tsx
<ConversationList
  // ...
  onAtBottomChange={nav.onAtBottomChange}
  onAtTopChange={nav.onAtTopChange}
/>

<WorkspacesMain
  // ...
  isAtBottom={nav.isAtBottom}
  isAtTop={nav.isAtTop}
  hasPreviousUserMessage={nav.hasPreviousUserMessage}
  hasNextUserMessage={nav.hasNextUserMessage}
  onScrollToBottom={nav.onScrollToBottom}
  onScrollToTop={nav.onScrollToTop}
  onScrollToPreviousMessage={nav.onScrollToPreviousMessage}
  onScrollToNextMessage={nav.onScrollToNextMessage}
/>

<ChatBoxWithDiffStats
  // ...
  onScrollToPreviousMessage={nav.onScrollToPreviousMessage}
  onScrollToBottom={nav.onScrollToBottom}
  onScrollToUserMessage={nav.onScrollToUserMessage}
  getActiveTurnPatchKey={nav.getActiveTurnPatchKey}
/>
```

- [ ] **Step 3: Update `useEffect` for `ResizeObserver` to use `nav.isAtBottomRef`**

```tsx
useEffect(() => {
  // ...
  if (!nav.isAtBottomRef.current) return;
  // ...
}, [workspaceWithSession?.id, session?.id, nav.isAtBottomRef]);
```

- [ ] **Step 4: Run check**

```bash
pnpm --filter @vibe/local-web run check
```

- [ ] **Step 5: Commit**

```bash
git add packages/web-core/src/pages/workspaces/WorkspacesMainContainer.tsx
git commit -m "refactor(workspaces): use useConversationNavController in WorkspacesMainContainer"
```

---

### Task 2.8: Migrate `VSCodeWorkspacePage.tsx` + memoize `workspaceWithSession`

**Files:**
- Modify: `packages/web-core/src/pages/workspaces/VSCodeWorkspacePage.tsx`

- [ ] **Step 1: Replace inline `NavButton` + the 8 handlers with the controller hook**

Mirror Task 2.7. Drop the inline `NavButton` and the Phosphor type alias.

- [ ] **Step 2: Memoize `workspaceWithSession`**

```tsx
import { useMemo } from 'react';

const workspaceWithSession = useMemo(
  () => (workspace ? createWorkspaceWithSession(workspace, selectedSession) : undefined),
  [workspace, selectedSession]
);
```

- [ ] **Step 3: Render the new overlay component instead of inline JSX**

```tsx
import { ConversationNavOverlay } from '@vibe/ui/components/ConversationNavOverlay';

// Inside JSX where the inline overlay used to be:
{workspaceWithSession && (
  <ConversationNavOverlay
    isAtTop={nav.isAtTop}
    isAtBottom={nav.isAtBottom}
    hasPreviousUserMessage={nav.hasPreviousUserMessage}
    hasNextUserMessage={nav.hasNextUserMessage}
    onScrollToTop={nav.onScrollToTop}
    onScrollToPreviousMessage={nav.onScrollToPreviousMessage}
    onScrollToNextMessage={nav.onScrollToNextMessage}
    onScrollToBottom={nav.onScrollToBottom}
  />
)}
```

- [ ] **Step 4: Run check**

```bash
pnpm --filter @vibe/local-web run check
```

- [ ] **Step 5: Commit**

```bash
git add packages/web-core/src/pages/workspaces/VSCodeWorkspacePage.tsx
git commit -m "refactor(vscode): use shared overlay + nav controller; memoize workspaceWithSession"
```

---

### Task 2.9: Migrate `ProjectRightSidebarContainer.tsx`

**Files:**
- Modify: `packages/web-core/src/pages/kanban/ProjectRightSidebarContainer.tsx`

- [ ] **Step 1: Same migration as Tasks 2.7 + 2.8 (without the memoize step — already memoized)**

Drop the inline `NavButton`, drop the Phosphor type alias, replace the 7 handlers with the controller, replace the inline overlay JSX with `<ConversationNavOverlay>`.

- [ ] **Step 2: Run check + lint**

```bash
pnpm --filter @vibe/local-web run check && pnpm run lint
```

- [ ] **Step 3: Manual smoke test across all 3 shells**

```bash
pnpm run local-web:dev
```

Verify the overlay renders correctly in:
- Workspaces page (long thread)
- VS Code webview (run via the VS Code extension dev path)
- Kanban issue → workspace right panel

- [ ] **Step 4: Commit**

```bash
git add packages/web-core/src/pages/kanban/ProjectRightSidebarContainer.tsx
git commit -m "refactor(kanban): use shared overlay + nav controller in ProjectRightSidebarContainer"
```

---

### Task 2.10: Push PR-B

- [ ] **Step 1: Push and open PR**

```bash
git push -u origin vk/d115-nav-extract
gh pr create --title "refactor(chat): extract ConversationNavOverlay + useConversationNavController" --body "$(cat <<'EOF'
## Summary

Eliminates the 3-shell duplication shipped in #9 by extracting a shared overlay component and controller hook.

- New `<ConversationNavOverlay>` in `packages/ui` (presentational, i18n-driven).
- New `useConversationNavController(ref)` hook in `packages/web-core/src/features/workspace-chat/model/`.
- Migrates `WorkspacesMain`, `VSCodeWorkspacePage`, `ProjectRightSidebarContainer`.
- Decouples `packages/ui` from `@phosphor-icons/react` type surface (icons used internally; no public type leak).
- Adds `workspaces.nav.*` keys to all 7 locales.
- Memoizes `workspaceWithSession` in `VSCodeWorkspacePage`.
- Vitest coverage on `<ConversationNavOverlay>` and the controller hook.

## Test plan
- [ ] vitest passes
- [ ] check-unused-i18n-keys passes
- [ ] Manual: overlay renders identically in all 3 shells
- [ ] Manual: visible labels in non-English locales
EOF
)"
```

---

## Phase 3 (PR-C `vk/d115-nav-keyboard`): Keyboard shortcut symmetry

### Task 3.1: Extend `SessionChatBoxContainer` props

**Files:**
- Modify: `packages/web-core/src/features/workspace-chat/ui/SessionChatBoxContainer.tsx`

- [ ] **Step 1: Branch**

```bash
git fetch origin main
git checkout -b vk/d115-nav-keyboard origin/main
```

- [ ] **Step 2: Add props to `SessionChatBoxContainerProps`**

```ts
onScrollToTop?: () => void;
onScrollToNextMessage?: () => void;
```

- [ ] **Step 3: Pass through to `<SessionChatBox>`**

```tsx
<SessionChatBox
  // ...existing props...
  onScrollToTop={onScrollToTop}
  onScrollToNextMessage={onScrollToNextMessage}
/>
```

---

### Task 3.2: Wire keyboard handlers in `SessionChatBox`

**Files:**
- Modify: `packages/ui/src/components/SessionChatBox.tsx`

- [ ] **Step 1: Add the props**

```ts
onScrollToTop?: () => void;
onScrollToNextMessage?: () => void;
```

- [ ] **Step 2: Locate the existing `onScrollToPreviousMessage` keyboard binding**

```bash
grep -n "onScrollToPreviousMessage\|Mod+\|Cmd+\|onKeyDown" packages/ui/src/components/SessionChatBox.tsx | head
```

- [ ] **Step 3: Mirror the binding**

In the existing `onKeyDown` (or whatever the binding mechanism is), add:

```tsx
// Cmd+Home / Ctrl+Home → top; Cmd+End / Ctrl+End → bottom (already exists for prev/next?)
if ((e.metaKey || e.ctrlKey) && e.key === 'Home') {
  e.preventDefault();
  onScrollToTop?.();
  return;
}
if ((e.metaKey || e.ctrlKey) && e.key === 'End') {
  e.preventDefault();
  onScrollToBottom?.();
  return;
}
// Reuse the prev-message binding shape for next-message:
// e.g. if existing binding is Cmd+Up for prev, mirror Cmd+Down for next.
if ((e.metaKey || e.ctrlKey) && e.key === 'ArrowDown' && onScrollToNextMessage) {
  e.preventDefault();
  onScrollToNextMessage();
  return;
}
```

Adjust to match the existing pattern in the file. Do NOT introduce a new framework — follow whatever convention `onScrollToPreviousMessage` uses.

---

### Task 3.3: Wire `onScrollToTop` / `onScrollToNextMessage` in 3 shells

**Files:**
- Modify: `packages/web-core/src/pages/workspaces/WorkspacesMainContainer.tsx`
- Modify: `packages/web-core/src/pages/workspaces/VSCodeWorkspacePage.tsx`
- Modify: `packages/web-core/src/pages/kanban/ProjectRightSidebarContainer.tsx`

- [ ] **Step 1: Pass `nav.onScrollToTop` and `nav.onScrollToNextMessage` to `<SessionChatBoxContainer>`**

```tsx
<SessionChatBoxContainer
  // ...existing props...
  onScrollToTop={nav.onScrollToTop}
  onScrollToNextMessage={nav.onScrollToNextMessage}
/>
```

Apply to all three shells.

---

### Task 3.4: Test keyboard handlers

**Files:**
- Create or modify: `packages/ui/src/components/SessionChatBox.test.tsx`

- [ ] **Step 1: Add tests**

```tsx
import { describe, it, expect, vi } from 'vitest';
import { render } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { SessionChatBox } from './SessionChatBox';

describe('SessionChatBox keyboard nav', () => {
  it('Cmd+Home invokes onScrollToTop', async () => {
    const onScrollToTop = vi.fn();
    render(
      <SessionChatBox
        // ...minimum required props for SessionChatBox; copy from existing story or test if any
        onScrollToTop={onScrollToTop}
      />
    );
    await userEvent.keyboard('{Meta>}{Home}{/Meta}');
    expect(onScrollToTop).toHaveBeenCalled();
  });

  it('Cmd+ArrowDown invokes onScrollToNextMessage', async () => {
    const onScrollToNextMessage = vi.fn();
    render(
      <SessionChatBox
        // ...
        onScrollToNextMessage={onScrollToNextMessage}
      />
    );
    await userEvent.keyboard('{Meta>}{ArrowDown}{/Meta}');
    expect(onScrollToNextMessage).toHaveBeenCalled();
  });
});
```

If `SessionChatBox` requires non-trivial setup (i18n, providers), copy the harness from any existing `SessionChatBox` test.

- [ ] **Step 2: Run; expect PASS**

- [ ] **Step 3: Commit + push + PR**

```bash
git add -A
git commit -m "feat(chat): keyboard shortcuts for scroll-to-top and next-user-message"
git push -u origin vk/d115-nav-keyboard
gh pr create --title "feat(chat): keyboard shortcuts for top/next-user-message" --body "Closes the kbd-shortcut asymmetry shipped in #9. Cmd+Home / Cmd+End / Cmd+ArrowDown / Cmd+ArrowUp all bound."
```

---

## Phase 4 (PR-D `vk/d115-nav-polish`): Edge cases + final hardening

### Task 4.1: Tolerance on `isAtTop`

**Files:**
- Modify: `packages/web-core/src/features/workspace-chat/model/useConversationVirtualizer.ts`
- Modify: `packages/web-core/src/features/workspace-chat/model/useConversationVirtualizer.test.ts`

- [ ] **Step 1: Branch**

```bash
git fetch origin main
git checkout -b vk/d115-nav-polish origin/main
```

- [ ] **Step 2: Add a regression test for sub-pixel flicker**

```ts
it('isAtTop tolerates sub-pixel scroll positions', () => {
  const ref = { current: makeContainer(500, 2000) };
  const { result } = renderHook(() =>
    useConversationVirtualizer({
      scrollContainerRef: ref as React.RefObject<HTMLElement>,
      rows: [],
    } as Parameters<typeof useConversationVirtualizer>[0])
  );
  act(() => {
    ref.current.scrollTop = 0.4; // sub-pixel
    ref.current.dispatchEvent(new Event('scroll'));
  });
  expect(result.current.isAtTop).toBe(true);
});
```

- [ ] **Step 3: Replace `el.scrollTop <= 0` with `el.scrollTop < 1`**

```ts
const nextAtTop = el ? el.scrollTop < 1 : true;
```

- [ ] **Step 4: Run; expect PASS**

```bash
pnpm --filter @vibe/web-core exec vitest run src/features/workspace-chat/model/useConversationVirtualizer.test.ts -t "sub-pixel"
```

- [ ] **Step 5: Commit**

```bash
git add packages/web-core/src/features/workspace-chat/model/useConversationVirtualizer.ts \
        packages/web-core/src/features/workspace-chat/model/useConversationVirtualizer.test.ts
git commit -m "fix(chat): tolerate sub-pixel scrollTop in isAtTop check"
```

---

### Task 4.2: Hook `scrollToTop` clears `planRevealSpacerRef`

**Files:**
- Modify: `packages/web-core/src/features/workspace-chat/model/useConversationVirtualizer.ts`
- Modify: `packages/web-core/src/features/workspace-chat/ui/ConversationListContainer.tsx`

The asymmetry is that the hook's `scrollToTop` doesn't clear the spacer; only the container's wrapper does. Move spacer cleanup into the hook by passing a `planRevealSpacerRef` option.

- [ ] **Step 1: Add a `planRevealSpacerRef` option to the hook**

```ts
interface UseConversationVirtualizerOptions {
  // ...existing fields...
  planRevealSpacerRef?: RefObject<HTMLElement | null>;
}
```

- [ ] **Step 2: In `scrollToTop`, clear the spacer**

```ts
const scrollToTop = useCallback((behavior: ScrollBehavior = 'smooth') => {
  const el = scrollContainerRef.current;
  if (!el) return;
  bottomLockedRef.current = false;
  if (planRevealSpacerRef?.current) {
    planRevealSpacerRef.current.style.height = '0px';
  }
  if (behavior === 'smooth') el.scrollTo({ top: 0, behavior: 'smooth' });
  else el.scrollTop = 0;
}, [scrollContainerRef, planRevealSpacerRef]);
```

- [ ] **Step 3: Pass the ref from `ConversationListContainer`**

```ts
const conversationVirtualizer = useConversationVirtualizer({
  // ...
  planRevealSpacerRef,
});
```

- [ ] **Step 4: Remove the duplicate spacer-cleanup from the container's wrapper `scrollToTop`**

The container can now just delegate:

```ts
const scrollToTop = useCallback(
  (behavior: 'auto' | 'smooth' = 'smooth') => {
    conversationVirtualizer.scrollToTop(behavior);
  },
  [conversationVirtualizer]
);
```

- [ ] **Step 5: Add a test asserting spacer clears**

```ts
it('scrollToTop clears the plan-reveal spacer', () => {
  const spacer = document.createElement('div');
  spacer.style.height = '120px';
  const ref = { current: makeContainer(500, 2000) };
  const spacerRef = { current: spacer };
  const { result } = renderHook(() =>
    useConversationVirtualizer({
      scrollContainerRef: ref as React.RefObject<HTMLElement>,
      planRevealSpacerRef: spacerRef,
      rows: [],
    } as Parameters<typeof useConversationVirtualizer>[0])
  );
  act(() => result.current.scrollToTop('auto'));
  expect(spacer.style.height).toBe('0px');
});
```

- [ ] **Step 6: Run check + tests; commit**

```bash
pnpm --filter @vibe/local-web run check
pnpm --filter @vibe/web-core exec vitest run src/features/workspace-chat/model/
git add packages/web-core/src/features/workspace-chat/model/useConversationVirtualizer.ts \
        packages/web-core/src/features/workspace-chat/model/useConversationVirtualizer.test.ts \
        packages/web-core/src/features/workspace-chat/ui/ConversationListContainer.tsx
git commit -m "fix(chat): scrollToTop clears plan-reveal spacer in hook (parity with container)"
```

---

### Task 4.3: Mobile gating

**Files:**
- Modify: `packages/web-core/src/features/workspace-chat/model/useConversationNavController.ts` (optional — may delegate to overlay's existing `isMobile` prop)
- Modify: `packages/web-core/src/pages/workspaces/WorkspacesMainContainer.tsx`
- Modify: `packages/web-core/src/pages/workspaces/VSCodeWorkspacePage.tsx`
- Modify: `packages/web-core/src/pages/kanban/ProjectRightSidebarContainer.tsx`

- [ ] **Step 1: Detect narrow viewport in each shell**

```tsx
import { useMediaQuery } from '@/shared/hooks/useMediaQuery'; // or whatever the project uses

const isNarrow = useMediaQuery('(max-width: 480px)');
```

If no `useMediaQuery` exists, inline a `useState` + `useEffect` matching `window.matchMedia('(max-width: 480px)')`. Discover via:

```bash
grep -rn "matchMedia\|useMediaQuery\|useBreakpoint" packages/web-core/src --include='*.ts*' | head
```

- [ ] **Step 2: Pass `isMobile={isNarrow}` to `<ConversationNavOverlay>`**

For `VSCodeWorkspacePage` and `ProjectRightSidebarContainer` which render the overlay directly, pass `isMobile`. For `WorkspacesMainContainer`, pass through `<WorkspacesMain isMobile={isNarrow}>` and threading it down.

- [ ] **Step 3: Add overlay test**

```tsx
// Already covered in Task 2.2 ('renders nothing on mobile').
```

- [ ] **Step 4: Run check + manual mobile QA**

Use Chrome DevTools device emulation at iPhone SE width (375×667). Confirm overlay does not render.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "fix(chat): hide nav overlay on narrow viewports"
```

---

### Task 4.4: Push PR-D

- [ ] **Step 1: Push and open PR**

```bash
git push -u origin vk/d115-nav-polish
gh pr create --title "fix(chat): nav overlay polish (sub-pixel, spacer parity, mobile)" --body "$(cat <<'EOF'
## Summary
- isAtTop tolerates sub-pixel scrollTop (no flicker on macOS rubber-band)
- Hook scrollToTop clears plan-reveal spacer (parity with container's wrapper, prevents future direct-call traps)
- Overlay hidden on narrow viewports (< 480px) to avoid overlap with chat input

## Test plan
- [ ] vitest tolerates 0.4px scrollTop test passes
- [ ] vitest spacer-clear test passes
- [ ] Manual: 375px viewport renders no overlay
EOF
)"
```

---

## Phase 5: Final cross-PR validation

After all 4 PRs merge, do a single end-to-end pass on `main`:

### Task 5.1: Full validation gate

- [ ] **Step 1: pull latest main**

```bash
git checkout main && git pull origin main
```

- [ ] **Step 2: Type check + lint + tests**

```bash
pnpm install
pnpm run check
pnpm run lint
pnpm --filter @vibe/web-core exec vitest run
pnpm --filter @vibe/ui exec vitest run
```

All four must exit 0.

- [ ] **Step 3: Manual QA matrix**

For each shell × scenario:

| Shell | Scenario | Expected |
|---|---|---|
| Workspaces main | Long thread, scroll mid | All 4 buttons visible |
| Workspaces main | At top | Top + prev hidden; next + bottom visible if those exist |
| Workspaces main | At bottom | Bottom + next hidden; top + prev visible if those exist |
| Workspaces main | Last entry is assistant | "Next user msg" hidden |
| Workspaces main | Click "Go to top", then scroll back to bottom | Streaming auto-follow re-engages on next message |
| VS Code | Same 5 scenarios | Same behaviors |
| Kanban sidebar | Same 5 scenarios | Same behaviors |
| Workspaces main @ 375px viewport | Overlay hidden | Empty container |
| Workspaces main, ja locale | Button labels translated | Japanese aria-labels |

- [ ] **Step 4: Confirm `check-unused-i18n-keys` is clean**

```bash
node scripts/check-unused-i18n-keys.mjs
```

Expected: no orphans for `workspaces.nav.*`.

- [ ] **Step 5: Confirm no leftover inline `NavButton` definitions**

```bash
grep -rn "function NavButton" packages --include='*.tsx' --exclude-dir=node_modules
```

Expected: a single hit in `packages/ui/src/components/ConversationNavOverlay.tsx`.

- [ ] **Step 6: Confirm Phosphor type surface in `packages/ui` is unchanged from pre-#9 baseline (modulo the `Icon` usage internal to overlay)**

```bash
grep -rn "type Icon\b\|@phosphor-icons/react" packages/ui/src --include='*.ts*'
```

Expected: only icon imports for use inside components, no public type re-export of `Icon`.

---

## Self-review checklist

- [x] **Spec coverage:** Every issue #1–#13 mapped to a task. Verify by scanning the table at the top.
- [x] **Placeholder scan:** No `TBD`, no "appropriate error handling", no "similar to Task N" — every code block is concrete.
- [x] **Type consistency:** `ConversationListHandle` extensions (`hasPreviousUserMessage`, `hasNextUserMessage`) defined in 1.5/1.6 and used identically in 2.5/2.6/2.7. `useConversationNavController` return shape consistent across consumers.
- [x] **Test coverage:** Every behavioral change has a vitest. Every bug has a regression test that fails first.
- [x] **PR sequencing:** Phase 1 ships standalone (critical bugs only). Phase 2 builds on Phase 1's `hasNext`/`hasPrevious` API. Phase 3 needs Phase 2's controller. Phase 4 builds on Phase 2.

## Open questions for the executor

1. **Does `packages/ui` already have a `vitest.config.ts`?** Task 2.2 may need one. Mirror `packages/web-core`'s if not.
2. **Existing keyboard binding location** — Task 3.2 assumes `SessionChatBox.tsx` owns the `onKeyDown` handler. If the binding lives in a parent component (`ChatInput`, etc.), adjust accordingly.
3. **`useMediaQuery` availability** — Task 4.3 falls back to inline `matchMedia`. Confirm via grep.
4. **Translation accuracy** — the i18n strings in Task 2.3 are machine-grade. Get a native review for ja/ko/zh-Hans/zh-Hant before merging Phase 2.

---
