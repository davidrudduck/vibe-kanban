# Filesystem Browser + Chat Card Enhancement — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a "Files" tab to the workspace that lets users browse the actual filesystem (worktree or main branch), view files with markdown/HTML preview, and jump to any file from existing chat diff cards via a new "Open in Files" icon button.

**Architecture:** New `crates/server/src/routes/workspaces/files.rs` handlers serve directory listings and file content (worktree via `std::fs`, main branch via `git show/ls-tree`). A Zustand store (`useFileBrowserStore`) holds selection state; React Query hooks fetch data. The Files tab renders as a split-pane container (`FileBrowserContainer`) inside the existing `rightMainPanelMode` panel slot in `WorkspacesLayout`. The chat diff card (`FileChangeRenderer`) gets a single icon button that sets the store state and switches to the Files tab.

**Tech Stack:** Rust/axum (backend), React + TypeScript (frontend), Zustand (state), TanStack Query (data fetching), `react-resizable-panels` (split pane), existing `MarkdownPreview` component, Phosphor icons

---

## File Map

**Create:**
- `crates/server/src/routes/workspaces/files.rs` — directory listing + file content handlers
- `packages/web-core/src/shared/stores/useFileBrowserStore.ts` — Zustand store
- `packages/web-core/src/shared/hooks/useFileBrowser.ts` — React Query hooks + API helpers
- `packages/ui/src/components/FileBrowserTreeNode.tsx` — single tree row (file or folder)
- `packages/web-core/src/pages/workspaces/FileBrowserCodeViewer.tsx` — syntax-highlighted code view
- `packages/web-core/src/pages/workspaces/FileBrowserMarkdownViewer.tsx` — markdown preview/raw toggle
- `packages/web-core/src/pages/workspaces/FileBrowserHtmlViewer.tsx` — sandboxed iframe + source toggle
- `packages/web-core/src/pages/workspaces/FileBrowserViewerPanel.tsx` — viewer header + mode toggle + sub-viewer
- `packages/web-core/src/pages/workspaces/FileBrowserTreePanel.tsx` — source toggle + filter + breadcrumb + tree list
- `packages/web-core/src/pages/workspaces/FileBrowserContainer.tsx` — split-pane root, wires store + queries

**Modify:**
- `crates/server/src/routes/workspaces/mod.rs` — add `mod files` + `.nest("/files", files::router())`
- `crates/server/src/bin/generate_types.rs` — export `DirectoryEntry`, `DirectoryListResponse`, `FileContentResponse`
- `packages/web-core/src/shared/stores/useUiPreferencesStore.ts` — add `FILES: 'files'` + `| 'files'` to `MobileTab`
- `packages/ui/src/components/Navbar.tsx` — add `'files'` to `MobileTabId` + entry in `MOBILE_TABS`
- `packages/web-core/src/shared/lib/api.ts` — add `listFiles` + `getFileContent` to `workspacesApi`
- `packages/web-core/src/pages/workspaces/WorkspacesLayout.tsx` — add Files panel (desktop + mobile)
- `packages/web-core/src/shared/components/NormalizedConversation/FileChangeRenderer.tsx` — add "Open in Files" button

---

## Task 1: Backend — Rust types + directory listing handler

**Files:**
- Create: `crates/server/src/routes/workspaces/files.rs`

- [ ] **Step 1: Create the file with types and list_directory handler**

```rust
// crates/server/src/routes/workspaces/files.rs
use axum::{
    Extension,
    Router,
    extract::{Query, State},
    routing::get,
};
use db::models::workspace::Workspace;
use db::models::workspace_repo::WorkspaceRepo;
use serde::{Deserialize, Serialize};
use std::path::Path;
use ts_rs::TS;

use crate::{
    error::ApiError,
    routes::common::{ApiResponse, ResponseJson},
    DeploymentImpl,
};

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DirectoryEntry {
    pub name: String,
    pub path: String,
    pub is_directory: bool,
    pub is_git_repo: bool,
    pub last_modified: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DirectoryListResponse {
    pub entries: Vec<DirectoryEntry>,
    pub current_path: String,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct FileContentResponse {
    pub path: String,
    pub content: String,
    pub size_bytes: u64,
    pub truncated: bool,
    pub language: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct FilesQuery {
    pub path: Option<String>,
    pub source: Option<String>,
}

fn detect_language(path: &str) -> Option<String> {
    let ext = Path::new(path).extension()?.to_str()?;
    let lang = match ext {
        "ts" | "tsx" => "typescript",
        "js" | "jsx" | "mjs" | "cjs" => "javascript",
        "rs" => "rust",
        "py" => "python",
        "go" => "go",
        "json" | "jsonl" => "json",
        "md" | "markdown" | "mdx" => "markdown",
        "toml" => "toml",
        "yaml" | "yml" => "yaml",
        "html" | "htm" => "xml",
        "css" => "css",
        "scss" => "scss",
        "sh" | "bash" | "zsh" => "bash",
        "sql" => "sql",
        "xml" => "xml",
        "graphql" | "gql" => "graphql",
        "dockerfile" => "dockerfile",
        "swift" => "swift",
        "kt" | "kts" => "kotlin",
        "java" => "java",
        "rb" => "ruby",
        "php" => "php",
        "cs" => "csharp",
        "cpp" | "cc" | "cxx" => "cpp",
        "c" | "h" => "c",
        _ => return None,
    };
    Some(lang.to_string())
}

#[axum::debug_handler]
pub async fn list_directory(
    Extension(workspace): Extension<Workspace>,
    State(deployment): State<DeploymentImpl>,
    Query(query): Query<FilesQuery>,
) -> Result<ResponseJson<ApiResponse<DirectoryListResponse>>, ApiError> {
    let pool = &deployment.db().pool;
    let repos = WorkspaceRepo::find_repos_for_workspace(pool, workspace.id).await?;
    let repo = repos
        .first()
        .ok_or_else(|| ApiError::BadRequest("Workspace has no repositories".to_string()))?;

    let container_ref = workspace
        .container_ref
        .as_deref()
        .ok_or_else(|| ApiError::BadRequest("Workspace container not available".to_string()))?;

    let worktree_root = Path::new(container_ref).join(&repo.name);
    let rel_path = query.path.as_deref().unwrap_or("");
    let source = query.source.as_deref().unwrap_or("worktree");

    let entries = match source {
        "main" => list_directory_git(&repo.path, rel_path)?,
        _ => list_directory_fs(&worktree_root, rel_path)?,
    };

    Ok(ResponseJson(ApiResponse {
        data: DirectoryListResponse {
            entries,
            current_path: rel_path.to_string(),
        },
    }))
}

fn list_directory_fs(
    worktree_root: &Path,
    rel_path: &str,
) -> Result<Vec<DirectoryEntry>, ApiError> {
    let target = worktree_root.join(rel_path);
    let canonical = target
        .canonicalize()
        .map_err(|_| ApiError::NotFound("Directory not found".to_string()))?;
    if !canonical.starts_with(worktree_root) {
        return Err(ApiError::BadRequest("Path traversal not allowed".to_string()));
    }
    if !canonical.is_dir() {
        return Err(ApiError::BadRequest("Path is not a directory".to_string()));
    }

    let mut entries: Vec<DirectoryEntry> = std::fs::read_dir(&canonical)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                return None;
            }
            let meta = e.metadata().ok()?;
            let path = if rel_path.is_empty() {
                name.clone()
            } else {
                format!("{}/{}", rel_path, name)
            };
            let is_directory = meta.is_dir();
            let is_git_repo = is_directory && e.path().join(".git").exists();
            let last_modified = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64);
            Some(DirectoryEntry {
                name,
                path,
                is_directory,
                is_git_repo,
                last_modified,
            })
        })
        .collect();

    entries.sort_by(|a, b| {
        b.is_directory
            .cmp(&a.is_directory)
            .then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    Ok(entries)
}

fn list_directory_git(repo_path: &str, rel_path: &str) -> Result<Vec<DirectoryEntry>, ApiError> {
    let tree_path = if rel_path.is_empty() {
        String::new()
    } else {
        format!("{}/" , rel_path)
    };

    let output = std::process::Command::new("git")
        .args(["ls-tree", "--long", "HEAD", &tree_path])
        .current_dir(repo_path)
        .output()
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    if !output.status.success() {
        return Err(ApiError::NotFound("Path not found in HEAD".to_string()));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut entries: Vec<DirectoryEntry> = stdout
        .lines()
        .filter_map(|line| {
            // format: "<mode> <type> <hash> <size>\t<name>"
            let tab = line.find('\t')?;
            let name = line[tab + 1..].to_string();
            let meta = &line[..tab];
            let parts: Vec<&str> = meta.split_whitespace().collect();
            let kind = parts.get(1)?;
            let is_directory = *kind == "tree";
            let path = if rel_path.is_empty() {
                name.clone()
            } else {
                format!("{}/{}", rel_path, name)
            };
            Some(DirectoryEntry {
                name,
                path,
                is_directory,
                is_git_repo: false,
                last_modified: None,
            })
        })
        .collect();

    entries.sort_by(|a, b| {
        b.is_directory
            .cmp(&a.is_directory)
            .then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    Ok(entries)
}

pub fn router() -> Router<DeploymentImpl> {
    Router::new()
        .route("/", get(list_directory))
        .route("/content", get(read_file))
}
```

- [ ] **Step 2: Add read_file handler to the same file** (append before `pub fn router()`)

```rust
#[axum::debug_handler]
pub async fn read_file(
    Extension(workspace): Extension<Workspace>,
    State(deployment): State<DeploymentImpl>,
    Query(query): Query<FilesQuery>,
) -> Result<ResponseJson<ApiResponse<FileContentResponse>>, ApiError> {
    let pool = &deployment.db().pool;
    let repos = WorkspaceRepo::find_repos_for_workspace(pool, workspace.id).await?;
    let repo = repos
        .first()
        .ok_or_else(|| ApiError::BadRequest("Workspace has no repositories".to_string()))?;

    let container_ref = workspace
        .container_ref
        .as_deref()
        .ok_or_else(|| ApiError::BadRequest("Workspace container not available".to_string()))?;

    let worktree_root = Path::new(container_ref).join(&repo.name);
    let rel_path = query
        .path
        .as_deref()
        .ok_or_else(|| ApiError::BadRequest("path query param required".to_string()))?;
    let source = query.source.as_deref().unwrap_or("worktree");

    const MAX_BYTES: u64 = 512 * 1024; // 500 KB

    let (bytes, size_bytes) = match source {
        "main" => read_file_git(&repo.path, rel_path)?,
        _ => read_file_fs(&worktree_root, rel_path)?,
    };

    let truncated = size_bytes > MAX_BYTES;
    let display_bytes = if truncated {
        &bytes[..MAX_BYTES as usize]
    } else {
        &bytes
    };

    // Return binary placeholder for non-UTF-8 content
    let content = String::from_utf8(display_bytes.to_vec()).unwrap_or_else(|_| {
        "__BINARY__".to_string()
    });

    Ok(ResponseJson(ApiResponse {
        data: FileContentResponse {
            path: rel_path.to_string(),
            content,
            size_bytes,
            truncated,
            language: detect_language(rel_path),
        },
    }))
}

fn read_file_fs(
    worktree_root: &Path,
    rel_path: &str,
) -> Result<(Vec<u8>, u64), ApiError> {
    let target = worktree_root.join(rel_path);
    let canonical = target
        .canonicalize()
        .map_err(|_| ApiError::NotFound("File not found".to_string()))?;
    if !canonical.starts_with(worktree_root) {
        return Err(ApiError::BadRequest("Path traversal not allowed".to_string()));
    }
    if canonical.is_dir() {
        return Err(ApiError::BadRequest("Path is a directory".to_string()));
    }
    let bytes = std::fs::read(&canonical).map_err(|e| ApiError::Internal(e.to_string()))?;
    let size = bytes.len() as u64;
    Ok((bytes, size))
}

fn read_file_git(repo_path: &str, rel_path: &str) -> Result<(Vec<u8>, u64), ApiError> {
    let output = std::process::Command::new("git")
        .args(["show", &format!("HEAD:{}", rel_path)])
        .current_dir(repo_path)
        .output()
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    if !output.status.success() {
        return Err(ApiError::NotFound("File not found in HEAD".to_string()));
    }

    let size = output.stdout.len() as u64;
    Ok((output.stdout, size))
}
```

- [ ] **Step 3: Verify it compiles**

```bash
cargo check -p server 2>&1 | head -40
```

Expected: no errors related to `files.rs` (some unresolved imports about `ResponseJson` / `ApiResponse` / `ApiError` will be fixed in Task 2 if needed — the exact import paths depend on the crate structure, check `crates/server/src/routes/workspaces/git.rs` lines 1-20 for the exact `use` statements to copy).

- [ ] **Step 4: Write Rust unit tests** (append to `files.rs`)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn make_tree(dir: &std::path::Path) -> TempDir {
        let tmp = TempDir::new_in(dir).unwrap();
        fs::write(tmp.path().join("alpha.ts"), "export {}").unwrap();
        fs::write(tmp.path().join("beta.md"), "# hello").unwrap();
        fs::create_dir(tmp.path().join("src")).unwrap();
        fs::write(tmp.path().join("src").join("main.rs"), "fn main(){}").unwrap();
        tmp
    }

    #[test]
    fn list_directory_fs_sorts_dirs_first_then_alpha() {
        let base = TempDir::new().unwrap();
        let tree = make_tree(base.path());
        let entries = list_directory_fs(base.path(), tree.path().file_name().unwrap().to_str().unwrap()).unwrap();
        // Actually list from the tree root
        let entries = list_directory_fs(tree.path(), "").unwrap();
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        // src dir must come before alpha.ts and beta.md
        assert_eq!(names[0], "src");
        assert!(names[1] < names[2]); // alpha < beta
    }

    #[test]
    fn list_directory_fs_rejects_traversal() {
        let tmp = TempDir::new().unwrap();
        let inner = tmp.path().join("workspace");
        fs::create_dir(&inner).unwrap();
        let result = list_directory_fs(&inner, "../");
        assert!(result.is_err());
    }

    #[test]
    fn detect_language_maps_extensions() {
        assert_eq!(detect_language("foo.ts"), Some("typescript".to_string()));
        assert_eq!(detect_language("bar.rs"), Some("rust".to_string()));
        assert_eq!(detect_language("baz.unknown"), None);
    }

    #[test]
    fn read_file_fs_rejects_traversal() {
        let tmp = TempDir::new().unwrap();
        let inner = tmp.path().join("workspace");
        fs::create_dir(&inner).unwrap();
        fs::write(tmp.path().join("secret.txt"), "secret").unwrap();
        let result = read_file_fs(&inner, "../secret.txt");
        assert!(result.is_err());
    }
}
```

- [ ] **Step 5: Run tests**

```bash
cargo test -p server files 2>&1 | tail -20
```

Expected: `test result: ok. N passed`

- [ ] **Step 6: Commit**

```bash
git add crates/server/src/routes/workspaces/files.rs
git commit -m "feat(server): add filesystem browser handlers (list_directory, read_file)"
```

---

## Task 2: Register routes + wire imports

**Files:**
- Modify: `crates/server/src/routes/workspaces/mod.rs`

- [ ] **Step 1: Add the files module and nest the router**

In `crates/server/src/routes/workspaces/mod.rs`, add `mod files;` at the top with the other module declarations, then add `.nest("/files", files::router())` inside `workspace_id_router`. The exact insertion point is the block that already has `.nest("/git", git::router())`, `.nest("/execution", execution::router())`, etc.:

```rust
// Add after the existing mod declarations at the top:
mod files;

// Inside the workspace_id_router builder, add after the existing .nest() calls:
.nest("/files", files::router())
```

- [ ] **Step 2: Verify compilation**

```bash
cargo check -p server 2>&1 | head -40
```

Expected: no errors

- [ ] **Step 3: Smoke-test the route exists**

```bash
cargo build -p server 2>&1 | tail -5
```

Expected: build succeeds

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/routes/workspaces/mod.rs
git commit -m "feat(server): register /workspaces/:id/files routes"
```

---

## Task 3: Export new types for TypeScript generation

**Files:**
- Modify: `crates/server/src/bin/generate_types.rs`

- [ ] **Step 1: Add the three new types to generate_types.rs**

Open `crates/server/src/bin/generate_types.rs` and find the block that calls `.export_to(...)` or `export_all_to(...)` for existing types. Add:

```rust
// Import the new types at the top of the file (with other use statements):
use server::routes::workspaces::files::{
    DirectoryEntry,
    DirectoryListResponse,
    FileContentResponse,
};

// In the export block (wherever other TS types are exported):
DirectoryEntry::export_all_to("../shared/types").unwrap();
DirectoryListResponse::export_all_to("../shared/types").unwrap();
FileContentResponse::export_all_to("../shared/types").unwrap();
```

(The exact pattern depends on what's already in the file — match the existing style exactly.)

- [ ] **Step 2: Make files.rs types pub**

In `crates/server/src/routes/workspaces/files.rs`, ensure the three structs are `pub` (they already are from Task 1) and that `mod files` in `mod.rs` is `pub mod files` if generate_types.rs imports from it. Check the existing pattern — if other route modules expose types, follow that pattern.

- [ ] **Step 3: Run type generation**

```bash
pnpm run generate-types 2>&1 | tail -10
```

Expected: exits 0, `shared/types.ts` now contains `DirectoryEntry`, `DirectoryListResponse`, `FileContentResponse` types.

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/bin/generate_types.rs shared/types.ts
git commit -m "feat: export DirectoryEntry, DirectoryListResponse, FileContentResponse to shared/types.ts"
```

---

## Task 4: Add FILES panel mode + MobileTab + Navbar entry

**Files:**
- Modify: `packages/web-core/src/shared/stores/useUiPreferencesStore.ts`
- Modify: `packages/ui/src/components/Navbar.tsx`

- [ ] **Step 1: Add FILES to RIGHT_MAIN_PANEL_MODES**

In `packages/web-core/src/shared/stores/useUiPreferencesStore.ts`, lines 6-11 (the `RIGHT_MAIN_PANEL_MODES` object):

```typescript
export const RIGHT_MAIN_PANEL_MODES = {
  CHANGES: 'changes',
  LOGS: 'logs',
  PREVIEW: 'preview',
  FILES: 'files',          // ← add this line
} as const;
```

- [ ] **Step 2: Add 'files' to MobileTab union**

In the same file, find `export type MobileTab =` (around line 17) and add `| 'files'`:

```typescript
export type MobileTab =
  | 'workspaces'
  | 'chat'
  | 'changes'
  | 'logs'
  | 'preview'
  | 'git'
  | 'files';           // ← add this line
```

- [ ] **Step 3: Add 'files' to MobileTabId and MOBILE_TABS in Navbar.tsx**

In `packages/ui/src/components/Navbar.tsx`, find `export type MobileTabId =` (around line 95) and add `| 'files'`. Then find `MOBILE_TABS` array (around line 103) and add the Files entry:

```typescript
export type MobileTabId =
  | 'workspaces'
  | 'chat'
  | 'changes'
  | 'logs'
  | 'files';          // ← add this line

// In MOBILE_TABS array, add after the 'logs' entry:
{ id: 'files' as MobileTabId, icon: FolderOpenIcon, label: 'Files' },
```

Add `FolderOpenIcon` to the Phosphor import at the top of `Navbar.tsx`:
```typescript
import { ..., FolderOpenIcon } from '@phosphor-icons/react';
```

- [ ] **Step 4: Type-check**

```bash
pnpm run check 2>&1 | grep -i "error\|Error" | head -20
```

Expected: no new errors

- [ ] **Step 5: Commit**

```bash
git add packages/web-core/src/shared/stores/useUiPreferencesStore.ts packages/ui/src/components/Navbar.tsx
git commit -m "feat: add FILES panel mode and mobile tab entry"
```

---

## Task 5: Add API methods to workspacesApi

**Files:**
- Modify: `packages/web-core/src/shared/lib/api.ts`

- [ ] **Step 1: Add listFiles and getFileContent to workspacesApi**

In `packages/web-core/src/shared/lib/api.ts`, find the `workspacesApi` object (around line 400). Add these two methods inside it, after the existing methods:

```typescript
  listFiles: async (
    workspaceId: string,
    path: string,
    source: 'worktree' | 'main'
  ): Promise<DirectoryListResponse> => {
    const params = new URLSearchParams({ path, source });
    const response = await makeRequest(
      `/api/workspaces/${workspaceId}/files?${params}`
    );
    return handleApiResponse<DirectoryListResponse>(response);
  },

  getFileContent: async (
    workspaceId: string,
    filePath: string,
    source: 'worktree' | 'main'
  ): Promise<FileContentResponse> => {
    const params = new URLSearchParams({ path: filePath, source });
    const response = await makeRequest(
      `/api/workspaces/${workspaceId}/files/content?${params}`
    );
    return handleApiResponse<FileContentResponse>(response);
  },
```

Add the necessary imports at the top of `api.ts` (if `DirectoryListResponse` and `FileContentResponse` aren't already imported from `shared/types`):

```typescript
import type {
  DirectoryListResponse,
  FileContentResponse,
  // ... existing imports
} from 'shared/types';
```

- [ ] **Step 2: Type-check**

```bash
pnpm run check 2>&1 | grep -i "error" | head -20
```

Expected: no new errors

- [ ] **Step 3: Commit**

```bash
git add packages/web-core/src/shared/lib/api.ts
git commit -m "feat: add listFiles and getFileContent to workspacesApi"
```

---

## Task 6: Zustand store + React Query hooks

**Files:**
- Create: `packages/web-core/src/shared/stores/useFileBrowserStore.ts`
- Create: `packages/web-core/src/shared/hooks/useFileBrowser.ts`

- [ ] **Step 1: Create the Zustand store**

```typescript
// packages/web-core/src/shared/stores/useFileBrowserStore.ts
import { create } from 'zustand';

export type FileSource = 'worktree' | 'main';
export type FileViewMode = 'preview' | 'raw' | 'rendered' | 'source' | null;

type FileBrowserState = {
  source: FileSource;
  currentPath: string | null;
  selectedFile: string | null;
  filterTerm: string;
  viewMode: FileViewMode;
  setSource: (source: FileSource) => void;
  navigate: (path: string | null) => void;
  selectFile: (path: string | null, viewMode?: FileViewMode) => void;
  setFilterTerm: (term: string) => void;
  setViewMode: (mode: FileViewMode) => void;
  openFile: (path: string) => void; // sets currentPath to parent, selects file, auto-picks viewMode
};

function autoViewMode(path: string): FileViewMode {
  const lower = path.toLowerCase();
  if (lower.endsWith('.md') || lower.endsWith('.markdown') || lower.endsWith('.mdx')) {
    return 'preview';
  }
  if (lower.endsWith('.html') || lower.endsWith('.htm')) {
    return 'rendered';
  }
  return null;
}

export const useFileBrowserStore = create<FileBrowserState>()((set) => ({
  source: 'worktree',
  currentPath: null,
  selectedFile: null,
  filterTerm: '',
  viewMode: null,

  setSource: (source) =>
    set({ source, currentPath: null, selectedFile: null, filterTerm: '' }),

  navigate: (path) =>
    set({ currentPath: path, selectedFile: null, filterTerm: '' }),

  selectFile: (path, viewMode) =>
    set({ selectedFile: path, viewMode: viewMode ?? null }),

  setFilterTerm: (filterTerm) => set({ filterTerm }),

  setViewMode: (viewMode) => set({ viewMode }),

  openFile: (path) => {
    const lastSlash = path.lastIndexOf('/');
    const parentPath = lastSlash > 0 ? path.slice(0, lastSlash) : null;
    set({
      currentPath: parentPath,
      selectedFile: path,
      source: 'worktree',
      viewMode: autoViewMode(path),
      filterTerm: '',
    });
  },
}));

// Atomic selectors to minimise re-renders
export const useFileBrowserSource = () => useFileBrowserStore((s) => s.source);
export const useFileBrowserCurrentPath = () => useFileBrowserStore((s) => s.currentPath);
export const useFileBrowserSelectedFile = () => useFileBrowserStore((s) => s.selectedFile);
export const useFileBrowserFilterTerm = () => useFileBrowserStore((s) => s.filterTerm);
export const useFileBrowserViewMode = () => useFileBrowserStore((s) => s.viewMode);
export const useFileBrowserActions = () =>
  useFileBrowserStore((s) => ({
    setSource: s.setSource,
    navigate: s.navigate,
    selectFile: s.selectFile,
    setFilterTerm: s.setFilterTerm,
    setViewMode: s.setViewMode,
    openFile: s.openFile,
  }));
```

- [ ] **Step 2: Create React Query hooks**

```typescript
// packages/web-core/src/shared/hooks/useFileBrowser.ts
import { useQuery } from '@tanstack/react-query';
import { workspacesApi } from '@/shared/lib/api';
import type { FileSource } from '@/shared/stores/useFileBrowserStore';

export const fileBrowserKeys = {
  directory: (id: string, path: string, source: FileSource) =>
    ['workspaceFiles', 'dir', id, source, path] as const,
  file: (id: string, path: string, source: FileSource) =>
    ['workspaceFiles', 'file', id, source, path] as const,
};

export function useDirectoryListing(
  workspaceId: string | undefined,
  path: string | null,
  source: FileSource
) {
  return useQuery({
    queryKey: fileBrowserKeys.directory(workspaceId ?? '', path ?? '', source),
    queryFn: () =>
      workspacesApi.listFiles(workspaceId!, path ?? '', source),
    enabled: !!workspaceId,
    staleTime: 30_000,
  });
}

export function useFileContent(
  workspaceId: string | undefined,
  filePath: string | null,
  source: FileSource
) {
  return useQuery({
    queryKey: fileBrowserKeys.file(workspaceId ?? '', filePath ?? '', source),
    queryFn: () =>
      workspacesApi.getFileContent(workspaceId!, filePath!, source),
    enabled: !!workspaceId && !!filePath,
    staleTime: 60_000,
  });
}

export function isMarkdownFile(path: string): boolean {
  return /\.(md|markdown|mdx)$/i.test(path);
}

export function isHtmlFile(path: string): boolean {
  return /\.(html|htm)$/i.test(path);
}
```

- [ ] **Step 3: Type-check**

```bash
pnpm run check 2>&1 | grep -i "error" | head -20
```

Expected: no new errors

- [ ] **Step 4: Commit**

```bash
git add packages/web-core/src/shared/stores/useFileBrowserStore.ts packages/web-core/src/shared/hooks/useFileBrowser.ts
git commit -m "feat: add FileBrowser Zustand store and React Query hooks"
```

---

## Task 7: FileBrowserTreeNode UI component

**Files:**
- Create: `packages/ui/src/components/FileBrowserTreeNode.tsx`

- [ ] **Step 1: Create the component**

```typescript
// packages/ui/src/components/FileBrowserTreeNode.tsx
import { CaretRightIcon, CaretDownIcon, FolderIcon, FileIcon } from '@phosphor-icons/react';
import { cn } from '../lib/cn';
import type { DirectoryEntry } from 'shared/types';

interface FileBrowserTreeNodeProps {
  entry: DirectoryEntry;
  depth?: number;
  isExpanded?: boolean;
  isSelected?: boolean;
  onClickFolder: (path: string) => void;
  onClickFile: (path: string) => void;
}

export function FileBrowserTreeNode({
  entry,
  depth = 0,
  isExpanded = false,
  isSelected = false,
  onClickFolder,
  onClickFile,
}: FileBrowserTreeNodeProps) {
  const indent = depth * 12;

  return (
    <button
      type="button"
      className={cn(
        'w-full flex items-center gap-1.5 px-2 py-1 text-left text-sm min-h-[32px] rounded-sm',
        'text-secondary-foreground hover:bg-secondary transition-colors',
        isSelected && 'bg-brand/10 text-brand'
      )}
      style={{ paddingLeft: `${8 + indent}px` }}
      onClick={() =>
        entry.is_directory ? onClickFolder(entry.path) : onClickFile(entry.path)
      }
    >
      {entry.is_directory ? (
        <>
          {isExpanded ? (
            <CaretDownIcon className="size-3 shrink-0 text-low" />
          ) : (
            <CaretRightIcon className="size-3 shrink-0 text-low" />
          )}
          <FolderIcon className="size-3.5 shrink-0 text-low" />
        </>
      ) : (
        <>
          <span className="w-3 shrink-0" />
          <FileIcon className="size-3.5 shrink-0 text-low" />
        </>
      )}
      <span className="truncate font-mono text-xs">{entry.name}</span>
    </button>
  );
}
```

- [ ] **Step 2: Export from ui package index** (check `packages/ui/src/index.ts` or wherever components are exported and add the new component)

```typescript
export { FileBrowserTreeNode } from './components/FileBrowserTreeNode';
```

- [ ] **Step 3: Type-check**

```bash
pnpm run check 2>&1 | grep -i "error" | head -20
```

- [ ] **Step 4: Commit**

```bash
git add packages/ui/src/components/FileBrowserTreeNode.tsx packages/ui/src/index.ts
git commit -m "feat(ui): add FileBrowserTreeNode component"
```

---

## Task 8: FileBrowserCodeViewer

**Files:**
- Create: `packages/web-core/src/pages/workspaces/FileBrowserCodeViewer.tsx`

- [ ] **Step 1: Create the component**

```typescript
// packages/web-core/src/pages/workspaces/FileBrowserCodeViewer.tsx
import { useMemo } from 'react';
import { getHighlightLanguage } from '@/shared/lib/extToLanguage';

interface FileBrowserCodeViewerProps {
  content: string;
  language: string | null;
}

export function FileBrowserCodeViewer({ content, language }: FileBrowserCodeViewerProps) {
  // Use a <pre><code> block — MarkdownPreview already ships highlight.js via rehype-highlight.
  // For the code viewer, render a fenced code block through a minimal wrapper.
  const fenced = useMemo(
    () => '```' + (language ?? '') + '\n' + content + '\n```',
    [content, language]
  );

  return (
    <div className="h-full overflow-auto font-mono text-xs">
      <pre className="p-4 whitespace-pre text-secondary-foreground">
        <code>{content}</code>
      </pre>
    </div>
  );
}
```

Note: syntax highlighting can be wired up in a follow-on — for the initial implementation, render as plain pre/code. The `language` field is passed through for future use with a highlight library.

- [ ] **Step 2: Type-check**

```bash
pnpm run check 2>&1 | grep -i "error" | head -20
```

- [ ] **Step 3: Commit**

```bash
git add packages/web-core/src/pages/workspaces/FileBrowserCodeViewer.tsx
git commit -m "feat: add FileBrowserCodeViewer component"
```

---

## Task 9: FileBrowserMarkdownViewer + FileBrowserHtmlViewer

**Files:**
- Create: `packages/web-core/src/pages/workspaces/FileBrowserMarkdownViewer.tsx`
- Create: `packages/web-core/src/pages/workspaces/FileBrowserHtmlViewer.tsx`

- [ ] **Step 1: Create FileBrowserMarkdownViewer**

```typescript
// packages/web-core/src/pages/workspaces/FileBrowserMarkdownViewer.tsx
import { MarkdownPreview } from '@/shared/components/MarkdownPreview';
import { FileBrowserCodeViewer } from './FileBrowserCodeViewer';
import type { FileViewMode } from '@/shared/stores/useFileBrowserStore';

interface FileBrowserMarkdownViewerProps {
  content: string;
  viewMode: FileViewMode;
}

export function FileBrowserMarkdownViewer({
  content,
  viewMode,
}: FileBrowserMarkdownViewerProps) {
  if (viewMode === 'raw') {
    return <FileBrowserCodeViewer content={content} language="markdown" />;
  }
  return (
    <div className="h-full overflow-auto p-4 prose prose-sm max-w-none dark:prose-invert">
      <MarkdownPreview content={content} />
    </div>
  );
}
```

- [ ] **Step 2: Create FileBrowserHtmlViewer**

```typescript
// packages/web-core/src/pages/workspaces/FileBrowserHtmlViewer.tsx
import { FileBrowserCodeViewer } from './FileBrowserCodeViewer';
import type { FileViewMode } from '@/shared/stores/useFileBrowserStore';

interface FileBrowserHtmlViewerProps {
  content: string;
  viewMode: FileViewMode;
}

export function FileBrowserHtmlViewer({
  content,
  viewMode,
}: FileBrowserHtmlViewerProps) {
  if (viewMode === 'source') {
    return <FileBrowserCodeViewer content={content} language="xml" />;
  }
  return (
    <iframe
      srcDoc={content}
      sandbox="allow-scripts"
      className="w-full h-full border-0 bg-white"
      title="HTML preview"
    />
  );
}
```

- [ ] **Step 3: Type-check**

```bash
pnpm run check 2>&1 | grep -i "error" | head -20
```

- [ ] **Step 4: Commit**

```bash
git add packages/web-core/src/pages/workspaces/FileBrowserMarkdownViewer.tsx packages/web-core/src/pages/workspaces/FileBrowserHtmlViewer.tsx
git commit -m "feat: add FileBrowserMarkdownViewer and FileBrowserHtmlViewer"
```

---

## Task 10: FileBrowserViewerPanel

**Files:**
- Create: `packages/web-core/src/pages/workspaces/FileBrowserViewerPanel.tsx`

- [ ] **Step 1: Create the component**

```typescript
// packages/web-core/src/pages/workspaces/FileBrowserViewerPanel.tsx
import { useTranslation } from 'react-i18next';
import { CopyIcon, SpinnerIcon } from '@phosphor-icons/react';
import { cn } from '@/shared/lib/utils';
import { FileBrowserCodeViewer } from './FileBrowserCodeViewer';
import { FileBrowserMarkdownViewer } from './FileBrowserMarkdownViewer';
import { FileBrowserHtmlViewer } from './FileBrowserHtmlViewer';
import { isMarkdownFile, isHtmlFile } from '@/shared/hooks/useFileBrowser';
import type { FileViewMode } from '@/shared/stores/useFileBrowserStore';
import type { FileContentResponse } from 'shared/types';

interface FileBrowserViewerPanelProps {
  selectedFile: string | null;
  fileData: FileContentResponse | undefined;
  isLoading: boolean;
  viewMode: FileViewMode;
  onSetViewMode: (mode: FileViewMode) => void;
}

export function FileBrowserViewerPanel({
  selectedFile,
  fileData,
  isLoading,
  viewMode,
  onSetViewMode,
}: FileBrowserViewerPanelProps) {
  if (!selectedFile) {
    return (
      <div className="flex-1 flex items-center justify-center text-low text-sm">
        Select a file to view
      </div>
    );
  }

  const fileName = selectedFile.split('/').pop() ?? selectedFile;
  const isMd = isMarkdownFile(selectedFile);
  const isHtml = isHtmlFile(selectedFile);
  const isBinary = fileData?.content === '__BINARY__';

  return (
    <div className="flex flex-col h-full min-h-0">
      {/* Header */}
      <div className="flex items-center gap-2 px-3 py-2 border-b border-border shrink-0">
        <span className="font-mono text-xs text-low truncate flex-1" title={selectedFile}>
          {selectedFile}
        </span>

        {/* Mode toggle */}
        {isMd && (
          <div className="flex gap-0.5">
            {(['preview', 'raw'] as FileViewMode[]).map((m) => (
              <button
                key={m}
                type="button"
                onClick={() => onSetViewMode(m)}
                className={cn(
                  'px-2 py-0.5 text-xs rounded border transition-colors',
                  viewMode === m
                    ? 'bg-secondary border-border text-normal'
                    : 'border-transparent text-low hover:text-normal'
                )}
              >
                {m === 'preview' ? 'Preview' : 'Raw'}
              </button>
            ))}
          </div>
        )}
        {isHtml && (
          <div className="flex gap-0.5">
            {(['rendered', 'source'] as FileViewMode[]).map((m) => (
              <button
                key={m}
                type="button"
                onClick={() => onSetViewMode(m)}
                className={cn(
                  'px-2 py-0.5 text-xs rounded border transition-colors',
                  viewMode === m
                    ? 'bg-secondary border-border text-normal'
                    : 'border-transparent text-low hover:text-normal'
                )}
              >
                {m === 'rendered' ? 'Rendered' : 'Source'}
              </button>
            ))}
          </div>
        )}

        <button
          type="button"
          title="Copy path"
          onClick={() => navigator.clipboard.writeText(selectedFile)}
          className="text-low hover:text-normal transition-colors"
        >
          <CopyIcon className="size-3.5" />
        </button>
      </div>

      {/* Body */}
      <div className="flex-1 min-h-0 overflow-hidden">
        {isLoading ? (
          <div className="flex items-center justify-center h-full">
            <SpinnerIcon className="size-5 animate-spin text-low" />
          </div>
        ) : isBinary ? (
          <div className="flex items-center justify-center h-full text-low text-sm">
            Binary file — cannot display
          </div>
        ) : fileData?.truncated ? (
          <div className="flex flex-col h-full">
            <div className="px-3 py-1.5 bg-warning/10 text-warning text-xs shrink-0">
              File truncated at 500 KB
            </div>
            <div className="flex-1 min-h-0 overflow-hidden">
              {renderContent(selectedFile, fileData.content, fileData.language ?? null, viewMode)}
            </div>
          </div>
        ) : fileData ? (
          renderContent(selectedFile, fileData.content, fileData.language ?? null, viewMode)
        ) : (
          <div className="flex items-center justify-center h-full text-low text-sm">
            File not found
          </div>
        )}
      </div>
    </div>
  );
}

function renderContent(
  path: string,
  content: string,
  language: string | null,
  viewMode: FileViewMode
) {
  if (isMarkdownFile(path)) {
    return <FileBrowserMarkdownViewer content={content} viewMode={viewMode} />;
  }
  if (isHtmlFile(path)) {
    return <FileBrowserHtmlViewer content={content} viewMode={viewMode} />;
  }
  return <FileBrowserCodeViewer content={content} language={language} />;
}
```

- [ ] **Step 2: Type-check**

```bash
pnpm run check 2>&1 | grep -i "error" | head -20
```

- [ ] **Step 3: Commit**

```bash
git add packages/web-core/src/pages/workspaces/FileBrowserViewerPanel.tsx
git commit -m "feat: add FileBrowserViewerPanel with mode toggle and binary/truncation handling"
```

---

## Task 11: FileBrowserTreePanel

**Files:**
- Create: `packages/web-core/src/pages/workspaces/FileBrowserTreePanel.tsx`

- [ ] **Step 1: Create the component**

```typescript
// packages/web-core/src/pages/workspaces/FileBrowserTreePanel.tsx
import { useMemo } from 'react';
import { SpinnerIcon, GitBranchIcon, FolderTreeIcon, MagnifyingGlassIcon } from '@phosphor-icons/react';
import { cn } from '@/shared/lib/utils';
import { FileBrowserTreeNode } from '@vibe/ui/components/FileBrowserTreeNode';
import type { DirectoryListResponse } from 'shared/types';
import type { FileSource } from '@/shared/stores/useFileBrowserStore';

interface FileBrowserTreePanelProps {
  workspaceId: string;
  listing: DirectoryListResponse | undefined;
  isLoading: boolean;
  source: FileSource;
  currentPath: string | null;
  selectedFile: string | null;
  filterTerm: string;
  onSetSource: (s: FileSource) => void;
  onNavigate: (path: string | null) => void;
  onSelectFile: (path: string) => void;
  onSetFilterTerm: (t: string) => void;
}

export function FileBrowserTreePanel({
  listing,
  isLoading,
  source,
  currentPath,
  selectedFile,
  filterTerm,
  onSetSource,
  onNavigate,
  onSelectFile,
  onSetFilterTerm,
}: FileBrowserTreePanelProps) {
  const filteredEntries = useMemo(() => {
    if (!listing) return [];
    const term = filterTerm.toLowerCase();
    const entries = term
      ? listing.entries.filter((e) => e.name.toLowerCase().includes(term))
      : listing.entries;
    return [...entries].sort((a, b) =>
      b.is_directory === a.is_directory
        ? a.name.localeCompare(b.name)
        : b.is_directory
          ? 1
          : -1
    );
  }, [listing, filterTerm]);

  const breadcrumbs = useMemo(() => {
    if (!currentPath) return [];
    return currentPath.split('/').filter(Boolean);
  }, [currentPath]);

  return (
    <div className="flex flex-col h-full min-h-0 border-r border-border">
      {/* Source toggle */}
      <div className="flex gap-1 p-2 shrink-0 border-b border-border">
        <button
          type="button"
          onClick={() => onSetSource('worktree')}
          className={cn(
            'flex-1 flex items-center justify-center gap-1 py-1 text-xs rounded transition-colors',
            source === 'worktree'
              ? 'bg-brand text-white'
              : 'bg-secondary text-low hover:text-normal'
          )}
        >
          <GitBranchIcon className="size-3" />
          Worktree
        </button>
        <button
          type="button"
          onClick={() => onSetSource('main')}
          className={cn(
            'flex-1 flex items-center justify-center gap-1 py-1 text-xs rounded transition-colors',
            source === 'main'
              ? 'bg-brand text-white'
              : 'bg-secondary text-low hover:text-normal'
          )}
        >
          <FolderTreeIcon className="size-3" />
          Main
        </button>
      </div>

      {/* Breadcrumb */}
      {currentPath && (
        <div className="flex items-center gap-0.5 px-2 py-1 text-xs text-low shrink-0 border-b border-border overflow-x-auto">
          <button
            type="button"
            onClick={() => onNavigate(null)}
            className="hover:text-normal shrink-0"
          >
            root
          </button>
          {breadcrumbs.map((crumb, i) => {
            const path = breadcrumbs.slice(0, i + 1).join('/');
            return (
              <span key={path} className="flex items-center gap-0.5 shrink-0">
                <span className="text-border">/</span>
                <button
                  type="button"
                  onClick={() => onNavigate(path)}
                  className="hover:text-normal"
                >
                  {crumb}
                </button>
              </span>
            );
          })}
        </div>
      )}

      {/* Filter */}
      <div className="px-2 py-1.5 shrink-0 border-b border-border">
        <div className="flex items-center gap-1.5 bg-secondary rounded px-2 py-1">
          <MagnifyingGlassIcon className="size-3 text-low shrink-0" />
          <input
            type="text"
            placeholder="Filter files…"
            value={filterTerm}
            onChange={(e) => onSetFilterTerm(e.target.value)}
            className="bg-transparent text-xs outline-none flex-1 text-normal placeholder:text-low"
          />
        </div>
      </div>

      {/* Tree */}
      <div className="flex-1 overflow-y-auto py-1">
        {isLoading ? (
          <div className="flex items-center justify-center py-8">
            <SpinnerIcon className="size-4 animate-spin text-low" />
          </div>
        ) : filteredEntries.length === 0 ? (
          <div className="px-3 py-4 text-xs text-low text-center">
            {filterTerm ? 'No matches' : 'Empty directory'}
          </div>
        ) : (
          filteredEntries.map((entry) => (
            <FileBrowserTreeNode
              key={entry.path}
              entry={entry}
              isSelected={selectedFile === entry.path}
              onClickFolder={(path) => onNavigate(path)}
              onClickFile={(path) => onSelectFile(path)}
            />
          ))
        )}
      </div>
    </div>
  );
}
```

- [ ] **Step 2: Type-check**

```bash
pnpm run check 2>&1 | grep -i "error" | head -20
```

- [ ] **Step 3: Commit**

```bash
git add packages/web-core/src/pages/workspaces/FileBrowserTreePanel.tsx
git commit -m "feat: add FileBrowserTreePanel with source toggle, breadcrumb, and filter"
```

---

## Task 12: FileBrowserContainer (root)

**Files:**
- Create: `packages/web-core/src/pages/workspaces/FileBrowserContainer.tsx`

- [ ] **Step 1: Create the split-pane container**

```typescript
// packages/web-core/src/pages/workspaces/FileBrowserContainer.tsx
import { Group, Panel, Separator } from 'react-resizable-panels';
import { FileBrowserTreePanel } from './FileBrowserTreePanel';
import { FileBrowserViewerPanel } from './FileBrowserViewerPanel';
import {
  useFileBrowserSource,
  useFileBrowserCurrentPath,
  useFileBrowserSelectedFile,
  useFileBrowserFilterTerm,
  useFileBrowserViewMode,
  useFileBrowserActions,
} from '@/shared/stores/useFileBrowserStore';
import { useDirectoryListing, useFileContent } from '@/shared/hooks/useFileBrowser';

interface FileBrowserContainerProps {
  workspaceId: string;
  className?: string;
}

export function FileBrowserContainer({
  workspaceId,
  className,
}: FileBrowserContainerProps) {
  const source = useFileBrowserSource();
  const currentPath = useFileBrowserCurrentPath();
  const selectedFile = useFileBrowserSelectedFile();
  const filterTerm = useFileBrowserFilterTerm();
  const viewMode = useFileBrowserViewMode();
  const { setSource, navigate, selectFile, setFilterTerm, setViewMode } =
    useFileBrowserActions();

  const { data: listing, isLoading: isListingLoading } = useDirectoryListing(
    workspaceId,
    currentPath,
    source
  );

  const { data: fileData, isLoading: isFileLoading } = useFileContent(
    workspaceId,
    selectedFile,
    source
  );

  return (
    <Group
      direction="horizontal"
      className={className ?? 'h-full min-h-0'}
      storage={{ getItem: () => null, setItem: () => {} }}
    >
      <Panel id="file-tree" defaultSize={35} minSize={20}>
        <FileBrowserTreePanel
          workspaceId={workspaceId}
          listing={listing}
          isLoading={isListingLoading}
          source={source}
          currentPath={currentPath}
          selectedFile={selectedFile}
          filterTerm={filterTerm}
          onSetSource={setSource}
          onNavigate={navigate}
          onSelectFile={(path) => selectFile(path)}
          onSetFilterTerm={setFilterTerm}
        />
      </Panel>

      <Separator
        id="file-browser-sep"
        className="w-1 bg-transparent hover:bg-brand/50 transition-colors cursor-col-resize"
      />

      <Panel id="file-viewer" defaultSize={65} minSize={30}>
        <FileBrowserViewerPanel
          selectedFile={selectedFile}
          fileData={fileData}
          isLoading={isFileLoading}
          viewMode={viewMode}
          onSetViewMode={setViewMode}
        />
      </Panel>
    </Group>
  );
}
```

- [ ] **Step 2: Type-check**

```bash
pnpm run check 2>&1 | grep -i "error" | head -20
```

- [ ] **Step 3: Commit**

```bash
git add packages/web-core/src/pages/workspaces/FileBrowserContainer.tsx
git commit -m "feat: add FileBrowserContainer split-pane root"
```

---

## Task 13: Wire Files tab into WorkspacesLayout

**Files:**
- Modify: `packages/web-core/src/pages/workspaces/WorkspacesLayout.tsx`

- [ ] **Step 1: Add import**

At the top of `WorkspacesLayout.tsx`, add:

```typescript
import { FileBrowserContainer } from './FileBrowserContainer';
```

- [ ] **Step 2: Add Files panel in desktop layout**

Find the `{rightMainPanelMode !== null && (<Panel id="right-main" ...>` block. Inside the Panel, after the Preview block, add:

```typescript
{rightMainPanelMode === RIGHT_MAIN_PANEL_MODES.FILES &&
  selectedWorkspace?.id && (
    <FileBrowserContainer
      workspaceId={selectedWorkspace.id}
      className="h-full min-h-0"
    />
  )}
```

- [ ] **Step 3: Add Files tab in mobile layout**

Find the `{/* Preview tab */}` block in the mobile layout section (around line 280). After the preview `</div>`, add:

```typescript
{/* Files tab */}
<div
  className={cn(
    'flex-1 min-h-0 overflow-hidden',
    mobileTab !== 'files' && 'hidden'
  )}
>
  {selectedWorkspace?.id && (
    <FileBrowserContainer
      workspaceId={selectedWorkspace.id}
      className="h-full min-h-0"
    />
  )}
</div>
```

- [ ] **Step 4: Type-check**

```bash
pnpm run check 2>&1 | grep -i "error" | head -20
```

Expected: no new errors

- [ ] **Step 5: Start dev server and verify the Files tab appears**

```bash
pnpm run dev
```

Open a workspace in the browser. The Files tab should now appear in the panel. Click it — the split-pane tree + viewer should render.

- [ ] **Step 6: Commit**

```bash
git add packages/web-core/src/pages/workspaces/WorkspacesLayout.tsx
git commit -m "feat: wire FileBrowserContainer into workspace Files tab (desktop + mobile)"
```

---

## Task 14: Chat file card — "Open in Files" button

**Files:**
- Modify: `packages/web-core/src/shared/components/NormalizedConversation/FileChangeRenderer.tsx`

- [ ] **Step 1: Add imports**

At the top of `FileChangeRenderer.tsx`, add:

```typescript
import { FolderOpenIcon } from 'lucide-react';
import { useFileBrowserStore } from '@/shared/stores/useFileBrowserStore';
import { useUiPreferencesStore, RIGHT_MAIN_PANEL_MODES } from '@/shared/stores/useUiPreferencesStore';
```

- [ ] **Step 2: Add the hook call inside the component**

Inside `FileChangeRenderer` (after the existing hook calls), add:

```typescript
const openFile = useFileBrowserStore((s) => s.openFile);
const setRightMainPanelMode = useUiPreferencesStore((s) => s.setRightMainPanelMode);
// workspaceId comes from context — check how other components get it in this file.
// If it's not available, read from the nearest workspace context or pass as prop.
```

Note: if `workspaceId` is not available in `FileChangeRenderer`, check how the existing component gets context (look at the props or context values already in the component). The `setRightMainPanelMode` action signature is `(mode, workspaceId) => void`.

- [ ] **Step 3: Add the button to the header row**

Find the header row (line ~135, the `<div className={headerClass}>` block). The current structure is:

```typescript
<div className={headerClass}>
  {icon}
  <p onClick={...} className="text-sm font-mono overflow-x-auto flex-1 cursor-pointer">
    {titleNode}
  </p>
</div>
```

Add the button between the `<p>` and the closing `</div>`:

```typescript
<button
  type="button"
  title="Open in Files"
  className="ml-auto shrink-0 p-1 rounded text-low hover:text-normal hover:bg-secondary transition-colors"
  onClick={(e) => {
    e.stopPropagation();
    openFile(path);
    setRightMainPanelMode(RIGHT_MAIN_PANEL_MODES.FILES, workspaceId);
  }}
>
  <FolderOpenIcon className="h-3.5 w-3.5" />
</button>
```

- [ ] **Step 4: Verify workspaceId is available**

Check the props/context of `FileChangeRenderer`. If `workspaceId` is not already available:
- Look at callers of `FileChangeRenderer` to see if `workspaceId` is passed
- Add it as a prop if missing: `workspaceId?: string` and update the callers

- [ ] **Step 5: Type-check**

```bash
pnpm run check 2>&1 | grep -i "error" | head -20
```

- [ ] **Step 6: Commit**

```bash
git add packages/web-core/src/shared/components/NormalizedConversation/FileChangeRenderer.tsx
git commit -m "feat: add Open in Files button to chat file diff cards"
```

---

## Task 15: Format, lint, type-check

- [ ] **Step 1: Format all code**

```bash
pnpm run format
```

- [ ] **Step 2: Full type-check**

```bash
pnpm run check 2>&1 | grep -i "error" | head -40
```

Expected: 0 errors

- [ ] **Step 3: Lint**

```bash
pnpm run lint 2>&1 | grep -i "error\|warning" | head -40
```

Fix any errors before continuing. Warnings are acceptable.

- [ ] **Step 4: Rust tests**

```bash
cargo test -p server files 2>&1 | tail -20
```

Expected: all tests pass

- [ ] **Step 5: Commit any formatting fixes**

```bash
git add -p  # stage only formatting changes
git commit -m "chore: format after filesystem browser implementation"
```

---

## Task 16: Manual golden path validation

- [ ] **Step 1: Start dev server**

```bash
pnpm run dev
```

- [ ] **Step 2: Files tab basics**
  - Open a workspace with an active agent
  - Confirm "Files" tab appears alongside Changes · Logs · Preview
  - Click Files tab → split-pane renders with tree on left, empty viewer on right
  - Tree shows root directory entries sorted folders-first

- [ ] **Step 3: Navigation**
  - Click a folder in the tree → breadcrumb updates, contents change
  - Click a breadcrumb segment → navigates back to that directory
  - Type in filter → only matching names shown

- [ ] **Step 4: File viewer**
  - Click a `.md` file → Preview mode active, markdown rendered
  - Toggle "Raw" → source code shown
  - Click a `.ts` file → code shown in pre/code block
  - Click an `.html` file → Rendered mode (iframe), toggle Source shows HTML

- [ ] **Step 5: Source toggle**
  - Click "Main" → tree reloads showing main branch files
  - Click "Worktree" → back to working tree

- [ ] **Step 6: Chat card button**
  - Open workspace chat
  - Find a file diff card (after an agent write operation)
  - Confirm folder icon appears in card header
  - Click it → Files tab activates, correct file selected in tree and viewer
  - For a `.md` file card → viewer opens in Preview mode automatically

- [ ] **Step 7: Edge cases**
  - Navigate to an empty directory → "Empty directory" message shown
  - Filter with no matches → "No matches" message shown
  - Very long path in breadcrumb → truncates gracefully

---

## Self-Review Checklist

All spec requirements mapped to tasks:
- ✅ Files tab (new RightMainPanelMode) → Tasks 4, 13
- ✅ Worktree/Main source toggle → Task 6 (store), 11 (UI)
- ✅ Filter input → Task 11
- ✅ Breadcrumb navigation → Task 11
- ✅ Split pane (tree + viewer) → Task 12
- ✅ `.md` preview/raw toggle → Tasks 9, 10
- ✅ `.html` rendered/source toggle → Tasks 9, 10
- ✅ Syntax highlighted code fallback → Task 8
- ✅ Backend API (list + read) → Tasks 1, 2
- ✅ Worktree path resolution → Task 1 (`container_ref` + `repo.name`)
- ✅ Main branch via git → Task 1 (`git ls-tree`, `git show`)
- ✅ Path traversal protection → Task 1 (canonicalize + prefix check)
- ✅ Truncation at 500 KB → Task 1, displayed in Task 10
- ✅ Binary file detection → Tasks 1 (`__BINARY__` sentinel), 10 (display)
- ✅ TypeScript type generation → Task 3
- ✅ Chat card "Open in Files" button → Task 14
- ✅ Auto view mode from chat card → Task 6 (`openFile` action)
- ✅ currentPath set to parent dir from chat card → Task 6
- ✅ Rust unit tests → Task 1
- ✅ format + check + lint → Task 15
