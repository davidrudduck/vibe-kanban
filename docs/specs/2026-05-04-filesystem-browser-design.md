# Design Spec: Filesystem Browser + Chat File Card Enhancement

**Date:** 2026-05-04  
**Status:** Approved  
**Reference:** vk-swarm at `/data/Code/vk-swarm` (gold standard)

---

## Overview

Two features ported from vk-swarm with full feature parity:

1. **Files tab** — a new workspace tab that lets users browse actual files in a worktree or main branch, with a split-pane tree + viewer (markdown preview, HTML render, syntax-highlighted code).
2. **Chat file card enhancement** — existing diff cards in the workspace chat get a small "Open in Files" icon button that switches to the Files tab and selects the file.

These two features compose: the Files tab is the viewer; the chat card icon is the entry point.

---

## Feature 1: Files Tab

### Placement

Added as a 4th tab in the workspace main panel alongside **Changes · Logs · Preview**. Implemented as a new tab option in `WorkspacesMainContainer` (or equivalent tab router).

### Layout

Split-pane: **tree panel (left, ~35% width, resizable) + viewer panel (right, ~65%)**. On narrow viewports, tree and viewer stack (drill-down navigation: tap folder enters it, tap file shows viewer with a back button).

### Tree Panel

- **Source toggle** at top: `Worktree` | `Main` — switches between the current working tree files and the main branch HEAD.
- **Filter input** — real-time substring match on file names. Case-insensitive. Folders first, then alphabetical.
- **Breadcrumb / drill-down** — shows current path; segments are clickable to navigate up. Root clears path.
- **Directory entries** — folders expand inline (desktop) or drill-down (mobile). Files are leaf nodes.
- **Tree state persistence** — collapsed folder paths persisted to `localStorage` keyed by `workspaceId + source`.

Tree node data shape (mirrors vk-swarm):
```typescript
type DirectoryEntry = {
  name: string;
  path: string;
  is_directory: boolean;
  is_git_repo: boolean;
  last_modified: number | null;
};

type DirectoryListResponse = {
  entries: DirectoryEntry[];
  current_path: string;
};
```

### Viewer Panel

Three rendering modes determined by file extension:

| Extension | Mode toggle | Default |
|-----------|-------------|---------|
| `.md`, `.markdown`, `.mdx` | Preview / Raw | Preview |
| `.html`, `.htm` | Rendered / Source | Rendered |
| All others | none (code only) | — |

**Markdown preview**: existing `MarkdownPreview` component (already in `packages/web-core/src/shared/components/MarkdownPreview.tsx`). Raw falls back to syntax-highlighted code view.

**HTML rendered**: sandboxed `<iframe srcDoc={content} sandbox="allow-scripts" />`. Source falls back to syntax-highlighted code view. Iframe should be sized to fill the viewer panel.

**Code/syntax highlighting**: existing `rehype-highlight` (already used in `MarkdownPreview`). Language detected from file extension. Line numbers displayed.

**Viewer header**: shows `path / filename`, mode toggle buttons (if applicable), and a copy-path button.

### State management

Zustand store (new, co-located with other workspace stores):
```typescript
type FileBrowserState = {
  source: 'worktree' | 'main';
  currentPath: string | null;
  selectedFile: string | null;
  filterTerm: string;
  viewMode: 'preview' | 'raw' | 'rendered' | 'source' | null;
  // actions
  setSource: (s: 'worktree' | 'main') => void;
  navigate: (path: string | null) => void;
  selectFile: (path: string | null) => void;
  setFilterTerm: (t: string) => void;
  setViewMode: (m: FileBrowserState['viewMode']) => void;
};
```

The chat card icon sets store state directly (switches active tab, sets `selectedFile`, sets `currentPath` to the file's parent directory). No URL params required — Zustand state is the source of truth for the Files tab within a session.

---

## Feature 2: Chat File Card Enhancement

### What changes

The existing file diff cards in the workspace chat panel (e.g. in `SessionChatBox` or wherever assistant tool-use file writes are rendered) get **one new icon button** added to the card header row.

- **Icon**: folder-open or external-link style (Phosphor `FolderOpen` or `ArrowSquareOut`)
- **Tooltip**: "Open in Files"
- **Size**: 26×26px icon button, same style as other action icons in the UI
- **Position**: right side of the card header, before the expand chevron

### Behaviour on click

1. Set workspace active tab to `Files`
2. Set `FileBrowserState.source` to `'worktree'` (default; main makes less sense for recently written files)
3. Set `FileBrowserState.selectedFile` to the file path from the card
4. Set `currentPath` to the file's parent directory (e.g. `src/routes` for `src/routes/mod.rs`) so the tree shows the right directory with the file visible
5. Auto-select correct view mode: `.md` → `'preview'`, `.html` → `'rendered'`, others → `null`

No new column, no inline viewer. The Files tab is the single viewer.

### What does NOT change

The existing diff card display (expand/collapse, green/red diff lines, file badge, `+N` stat) is untouched.

---

## Backend API

New workspace-scoped routes in `crates/server/src/routes/workspaces/`:

```text
GET /api/workspaces/:id/files?path=<dir>&source=<worktree|main>
→ DirectoryListResponse

GET /api/workspaces/:id/files/content?path=<file>&source=<worktree|main>
→ FileContentResponse { path, content, size_bytes, truncated, language }
```

**Worktree source**: resolve path relative to the workspace's working directory on disk.  
**Main source**: use `git show HEAD:<path>` to read file content; directory listing via `git ls-tree HEAD <path>`.  
**Truncation**: files over 500 KB are truncated; `truncated: true` is sent to the client which shows a warning banner.  
**Security**: paths are validated to prevent directory traversal (canonicalize and assert prefix matches workspace root).

Language detection: map common extensions to highlight.js language identifiers server-side; include in `FileContentResponse.language`.

---

## Component Architecture

```text
packages/web-core/src/pages/workspaces/
  FileBrowserContainer.tsx       ← new: tab content root, wires store + queries
  FileBrowserTreePanel.tsx       ← new: tree + filter + breadcrumb + source toggle
  FileBrowserViewerPanel.tsx     ← new: viewer header + mode toggle + renders
  FileBrowserMarkdownViewer.tsx  ← new: wraps existing MarkdownPreview
  FileBrowserHtmlViewer.tsx      ← new: sandboxed iframe
  FileBrowserCodeViewer.tsx      ← new: syntax-highlighted code + line numbers

packages/web-core/src/shared/stores/
  useFileBrowserStore.ts         ← new: Zustand store

packages/ui/src/components/
  FileBrowserTreeNode.tsx        ← new: single tree entry (file or folder row)
  FileBrowserSearchBar.tsx       ← new: filter input + breadcrumb row
```

Existing components untouched: `FileTree.tsx`, `FileTreeContainer.tsx` (diff tree stays as-is in Changes tab).

Chat card change: `packages/web-core/src/shared/components/NormalizedConversation/FileChangeRenderer.tsx` — add the icon button to the existing card header row.

---

## Testing & Validation

### Unit tests (Rust — `cargo test`)
- `workspaces/files`: directory listing returns sorted entries (dirs first, then alpha)
- `workspaces/files`: path traversal attempts return 400
- `workspaces/files`: files over 500 KB return `truncated: true`
- `workspaces/files`: `source=main` reads from git HEAD, not working tree
- `workspaces/files`: missing workspace returns 404

### Frontend validation (manual golden path)
1. Navigate to a workspace → Files tab visible in tab bar
2. Tree loads root directory entries for worktree
3. Click a folder → contents load, breadcrumb updates
4. Filter input → only matching files shown, dirs first
5. Click a `.md` file → Preview mode active, markdown rendered
6. Toggle Raw → source code shown with syntax highlighting
7. Click an `.html` file → Rendered mode active, iframe shows rendered page
8. Toggle Source → HTML source shown
9. Source toggle to Main → tree reloads from main branch
10. Open any workspace chat → file diff cards have folder icon in header
11. Click folder icon → Files tab activates, correct file selected, correct view mode

### Edge cases to validate
- Binary files (images, PDFs) → show "Binary file — cannot display" message
- Empty directories → show "Empty directory" state
- File deleted between list and content fetch → show "File not found" error
- Very long file paths → breadcrumb truncates gracefully
- No workspace worktree (remote workspace) → worktree source disabled, Main only

### Type safety
- `pnpm run check` must pass (frontend TypeScript + Rust)
- `pnpm run lint` must pass (ESLint + Clippy)
- `pnpm run generate-types` must be re-run if new Rust types are added (new `DirectoryEntry`, `FileContentResponse` structs with `#[derive(TS)]`)

---

## Out of Scope

- File editing in the viewer (read-only)
- File upload / download
- Search within file content (Ctrl+F)
- Git blame / history view
- Keyboard shortcut to open Files tab (can be added separately)
