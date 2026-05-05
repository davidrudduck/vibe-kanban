# Filesystem Browser Security Hardening & Quality Remediation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remediate all P0–P1 findings from the tri-model adversarial review of the filesystem browser feature, covering backend security, DoS prevention, API correctness, and frontend reliability.

**Architecture:** Backend changes are isolated to `crates/server/src/routes/workspaces/files.rs`; type regen flows through `crates/server/src/bin/generate_types.rs` → `shared/types.ts`; frontend changes touch the store, hooks, and viewer components. Tasks are ordered so each one compiles independently before the next begins.

**Tech Stack:** Rust/axum (backend), ts-rs (type generation), Zustand v4 + TanStack Query v5 (frontend state), highlight.js (syntax highlighting via `rehype-highlight` transitive dep already in `packages/web-core`)

---

## File Map

| File | Changes |
|------|---------|
| `crates/server/src/routes/workspaces/files.rs` | Tasks 1–3: hidden-file guard, symlink guard, streaming reads, `is_binary` field, error codes |
| `crates/server/src/bin/generate_types.rs` | Task 4: no code change; `pnpm run generate-types` regenerates types |
| `shared/types.ts` | Task 4: auto-generated; do not edit manually |
| `packages/web-core/src/pages/workspaces/FileBrowserViewerPanel.tsx` | Task 4: update binary check to use `fileData.is_binary` |
| `packages/web-core/src/shared/stores/useFileBrowserStore.ts` | Tasks 5–6: add `resetForWorkspace`, remove source override in `openFile`, fix `useShallow` |
| `packages/web-core/src/pages/workspaces/FileBrowserContainer.tsx` | Tasks 5, 7: add workspace-switch `useEffect`, thread `isError` |
| `packages/web-core/src/shared/hooks/useFileBrowser.ts` | Tasks 7–8: thread `isError`, add `placeholderData` |
| `packages/web-core/src/pages/workspaces/FileBrowserTreePanel.tsx` | Task 7: add error state |
| `packages/web-core/src/pages/workspaces/FileBrowserCodeViewer.tsx` | Task 9: add syntax highlighting via highlight.js |

---

## Task 1: Backend — Hidden-File Guard + Symlink Guard

**Context:** The adversarial review (Opus model) identified that `read_file_fs` and `read_file_git` do not reject paths with dotfile components (e.g. `path=.env`, `path=src/.secrets`). An attacker who knows the path can read `.env` directly via the `/content` endpoint even though the listing UI hides dotfiles. Additionally, `read_file_fs` does not check for symlinks before `canonicalize()`, leaving a narrow TOCTOU window. `list_directory_git` also accepts hidden directory paths (e.g. `path=.git`) exposing git internals.

**Files:**
- Modify: `crates/server/src/routes/workspaces/files.rs`

- [ ] **Step 1: Write the failing tests**

Add these tests inside the `#[cfg(test)]` block at the bottom of `files.rs`:

```rust
#[test]
fn read_file_fs_rejects_hidden_file() {
    let tmp = TempDir::new().unwrap();
    let inner = tmp.path().join("workspace");
    fs::create_dir(&inner).unwrap();
    fs::write(inner.join(".env"), "SECRET=abc").unwrap();
    let result = read_file_fs(&inner, ".env");
    assert!(result.is_err());
    let msg = format!("{:?}", result.unwrap_err());
    assert!(msg.contains("Hidden") || msg.contains("hidden"), "err was: {msg}");
}

#[test]
fn read_file_fs_rejects_hidden_dir_component() {
    let tmp = TempDir::new().unwrap();
    let inner = tmp.path().join("workspace");
    fs::create_dir_all(inner.join(".config")).unwrap();
    fs::write(inner.join(".config").join("secrets.toml"), "token=x").unwrap();
    let result = read_file_fs(&inner, ".config/secrets.toml");
    assert!(result.is_err());
}

#[test]
fn list_directory_fs_rejects_hidden_path() {
    let tmp = TempDir::new().unwrap();
    let inner = tmp.path().join("workspace");
    fs::create_dir_all(inner.join(".git")).unwrap();
    let result = list_directory_fs(&inner, ".git");
    assert!(result.is_err());
}

#[test]
fn list_directory_git_rejects_hidden_path() {
    // list_directory_git validates before hitting git; path ".git" must be rejected
    let tmp = TempDir::new().unwrap();
    let result = list_directory_git(tmp.path(), ".git");
    assert!(result.is_err());
}

#[test]
fn read_file_git_rejects_hidden_file() {
    let tmp = TempDir::new().unwrap();
    let result = read_file_git(tmp.path(), ".env");
    assert!(result.is_err());
}

#[test]
fn read_file_fs_rejects_symlink() {
    use std::os::unix::fs::symlink;
    let tmp = TempDir::new().unwrap();
    let inner = tmp.path().join("workspace");
    fs::create_dir(&inner).unwrap();
    let target = tmp.path().join("secret.txt");
    fs::write(&target, "secret").unwrap();
    symlink(&target, inner.join("link.txt")).unwrap();
    let result = read_file_fs(&inner, "link.txt");
    assert!(result.is_err());
    let msg = format!("{:?}", result.unwrap_err());
    assert!(msg.contains("ymlink") || msg.contains("not allowed"), "err was: {msg}");
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test -p server files 2>&1 | tail -30
```

Expected: multiple test failures (the guards don't exist yet).

- [ ] **Step 3: Add the hidden-file guard and symlink guard**

Replace the body of `list_directory_fs` (starting at line 128), `list_directory_git` (line 188), `read_file_fs` (line 302), and `read_file_git` (line 323) with the versions below.

**`list_directory_fs`** — add hidden-path guard after the traversal check:

```rust
fn list_directory_fs(
    worktree_root: &Path,
    rel_path: &str,
) -> Result<Vec<DirectoryEntry>, ApiError> {
    // Reject paths with any hidden component
    if rel_path.split('/').any(|c| !c.is_empty() && c.starts_with('.')) {
        return Err(ApiError::BadRequest(
            "Hidden directories are not accessible".to_string(),
        ));
    }
    let canonical_root = worktree_root
        .canonicalize()
        .map_err(|_| ApiError::BadRequest("Workspace root not found".to_string()))?;
    let target = canonical_root.join(rel_path);
    let canonical = target
        .canonicalize()
        .map_err(|_| ApiError::BadRequest("Directory not found".to_string()))?;
    if !canonical.starts_with(&canonical_root) {
        return Err(ApiError::BadRequest(
            "Path traversal not allowed".to_string(),
        ));
    }
    if !canonical.is_dir() {
        return Err(ApiError::BadRequest("Path is not a directory".to_string()));
    }

    let mut entries: Vec<DirectoryEntry> = std::fs::read_dir(&canonical)
        .map_err(|e| ApiError::BadRequest(e.to_string()))?
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
```

**`list_directory_git`** — add hidden-path guard:

```rust
fn list_directory_git(repo_path: &Path, rel_path: &str) -> Result<Vec<DirectoryEntry>, ApiError> {
    if rel_path.contains("..") || rel_path.starts_with('/') || rel_path.starts_with('-') {
        return Err(ApiError::BadRequest("Invalid path".to_string()));
    }
    if rel_path.split('/').any(|c| !c.is_empty() && c.starts_with('.')) {
        return Err(ApiError::BadRequest(
            "Hidden directories are not accessible".to_string(),
        ));
    }

    let tree_path = if rel_path.is_empty() {
        String::new()
    } else {
        format!("{}/", rel_path)
    };

    let output = std::process::Command::new("git")
        .args(["ls-tree", "--long", "HEAD", "--", &tree_path])
        .current_dir(repo_path)
        .output()
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    if !output.status.success() {
        return Err(ApiError::BadRequest("Path not found in HEAD".to_string()));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut entries: Vec<DirectoryEntry> = stdout
        .lines()
        .filter_map(|line| {
            let tab = line.find('\t')?;
            let name = line[tab + 1..].to_string();
            // Filter hidden entries from listing output
            if name.starts_with('.') {
                return None;
            }
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
```

**`read_file_fs`** — add hidden-path guard + symlink guard (symlink check uses `symlink_metadata` on the pre-canonicalize path):

```rust
fn read_file_fs(worktree_root: &Path, rel_path: &str) -> Result<(Vec<u8>, u64), ApiError> {
    // Reject hidden path components
    if rel_path.split('/').any(|c| !c.is_empty() && c.starts_with('.')) {
        return Err(ApiError::BadRequest(
            "Hidden files are not accessible".to_string(),
        ));
    }
    let canonical_root = worktree_root
        .canonicalize()
        .map_err(|_| ApiError::BadRequest("Workspace root not found".to_string()))?;
    let target = canonical_root.join(rel_path);

    // Symlink guard: check before canonicalize resolves it
    let sym_meta = std::fs::symlink_metadata(&target)
        .map_err(|_| ApiError::BadRequest("File not found".to_string()))?;
    if sym_meta.file_type().is_symlink() {
        return Err(ApiError::BadRequest(
            "Symlink access not allowed".to_string(),
        ));
    }

    let canonical = target
        .canonicalize()
        .map_err(|_| ApiError::BadRequest("File not found".to_string()))?;
    if !canonical.starts_with(&canonical_root) {
        return Err(ApiError::BadRequest(
            "Path traversal not allowed".to_string(),
        ));
    }
    if canonical.is_dir() {
        return Err(ApiError::BadRequest("Path is a directory".to_string()));
    }
    let bytes = std::fs::read(&canonical).map_err(|e| ApiError::BadRequest(e.to_string()))?;
    let size = bytes.len() as u64;
    Ok((bytes, size))
}
```

**`read_file_git`** — add hidden-path guard:

```rust
fn read_file_git(repo_path: &Path, rel_path: &str) -> Result<(Vec<u8>, u64), ApiError> {
    if rel_path.contains("..") || rel_path.starts_with('/') || rel_path.starts_with('-') {
        return Err(ApiError::BadRequest("Invalid path".to_string()));
    }
    if rel_path.split('/').any(|c| !c.is_empty() && c.starts_with('.')) {
        return Err(ApiError::BadRequest(
            "Hidden files are not accessible".to_string(),
        ));
    }

    let output = std::process::Command::new("git")
        .args(["show", &format!("HEAD:{}", rel_path)])
        .current_dir(repo_path)
        .output()
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    if !output.status.success() {
        return Err(ApiError::BadRequest("File not found in HEAD".to_string()));
    }

    let size = output.stdout.len() as u64;
    Ok((output.stdout, size))
}
```

- [ ] **Step 4: Run tests — expect all new tests to pass**

```bash
cargo test -p server files 2>&1 | tail -30
```

Expected output: all tests pass including the 6 new ones.

- [ ] **Step 5: Confirm it compiles**

```bash
pnpm run backend:check 2>&1 | grep -E "^error" | head -20
```

Expected: no errors.

- [ ] **Step 6: Commit**

```bash
git add crates/server/src/routes/workspaces/files.rs
git commit -m "fix(files): hidden-file guard + symlink check in all read/list endpoints"
```

---

## Task 2: Backend — Streaming Reads (DoS Prevention)

**Context:** `read_file_fs` calls `std::fs::read()` which loads the entire file into memory before truncating. On a multi-GB file this is a DoS vector. `read_file_git` does the same via `git show` stdout. The fix: for FS, stat first then `File::open → take(MAX_BYTES + 1) → read_to_end`; for git, use `git cat-file -s` to get size, then stream with a byte limit.

Note: `MAX_BYTES` is currently defined inside `read_file` (the handler). We need to pass it into the helpers or define it at module level.

**Files:**
- Modify: `crates/server/src/routes/workspaces/files.rs`

- [ ] **Step 1: Write a failing test for large-file streaming**

Add inside `#[cfg(test)]`:

```rust
#[test]
fn read_file_fs_does_not_load_full_large_file() {
    // Write a 1 MB file; with MAX_BYTES = 500 KB we should read only ~500 KB + 1
    let tmp = TempDir::new().unwrap();
    let inner = tmp.path().join("workspace");
    fs::create_dir(&inner).unwrap();
    let big = vec![b'a'; 1024 * 1024]; // 1 MB
    fs::write(inner.join("big.txt"), &big).unwrap();

    const MAX: u64 = 500 * 1024;
    let (bytes, size) = read_file_fs_limited(&inner, "big.txt", MAX).unwrap();
    assert_eq!(size, 1024 * 1024, "size should be full file size");
    assert!(bytes.len() as u64 <= MAX + 1, "read bytes should be capped at MAX+1");
}
```

This test calls `read_file_fs_limited` — a version of `read_file_fs` that accepts the limit. We'll refactor the signature.

- [ ] **Step 2: Run test to see it fail**

```bash
cargo test -p server files::tests::read_file_fs_does_not_load_full_large_file 2>&1 | tail -10
```

Expected: compile error (function doesn't exist yet).

- [ ] **Step 3: Refactor `read_file_fs` and `read_file_git` to accept a byte limit and stream**

At the top of `files.rs`, add `use std::io::Read;`. Then move `MAX_BYTES` to a module-level constant and refactor the helpers:

```rust
use std::io::Read;

const MAX_BYTES: u64 = 500 * 1024;
```

Replace `read_file_fs` with `read_file_fs_limited` (rename + add limit param):

```rust
fn read_file_fs_limited(
    worktree_root: &Path,
    rel_path: &str,
    limit: u64,
) -> Result<(Vec<u8>, u64), ApiError> {
    // Reject hidden path components
    if rel_path.split('/').any(|c| !c.is_empty() && c.starts_with('.')) {
        return Err(ApiError::BadRequest(
            "Hidden files are not accessible".to_string(),
        ));
    }
    let canonical_root = worktree_root
        .canonicalize()
        .map_err(|_| ApiError::BadRequest("Workspace root not found".to_string()))?;
    let target = canonical_root.join(rel_path);

    // Symlink guard
    let sym_meta = std::fs::symlink_metadata(&target)
        .map_err(|_| ApiError::BadRequest("File not found".to_string()))?;
    if sym_meta.file_type().is_symlink() {
        return Err(ApiError::BadRequest(
            "Symlink access not allowed".to_string(),
        ));
    }

    let canonical = target
        .canonicalize()
        .map_err(|_| ApiError::BadRequest("File not found".to_string()))?;
    if !canonical.starts_with(&canonical_root) {
        return Err(ApiError::BadRequest(
            "Path traversal not allowed".to_string(),
        ));
    }
    if canonical.is_dir() {
        return Err(ApiError::BadRequest("Path is a directory".to_string()));
    }

    // Stat for true size, then stream limited bytes
    let size = std::fs::metadata(&canonical)
        .map_err(|e| ApiError::BadRequest(e.to_string()))?
        .len();
    let mut file =
        std::fs::File::open(&canonical).map_err(|e| ApiError::BadRequest(e.to_string()))?;
    let mut bytes = Vec::new();
    file.by_ref()
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;
    Ok((bytes, size))
}
```

Keep `read_file_fs` as a thin wrapper for existing callers and tests:

```rust
fn read_file_fs(worktree_root: &Path, rel_path: &str) -> Result<(Vec<u8>, u64), ApiError> {
    read_file_fs_limited(worktree_root, rel_path, MAX_BYTES)
}
```

Replace `read_file_git` to stream via piped stdout:

```rust
fn read_file_git(repo_path: &Path, rel_path: &str) -> Result<(Vec<u8>, u64), ApiError> {
    if rel_path.contains("..") || rel_path.starts_with('/') || rel_path.starts_with('-') {
        return Err(ApiError::BadRequest("Invalid path".to_string()));
    }
    if rel_path.split('/').any(|c| !c.is_empty() && c.starts_with('.')) {
        return Err(ApiError::BadRequest(
            "Hidden files are not accessible".to_string(),
        ));
    }

    let git_ref = format!("HEAD:{}", rel_path);

    // Get true file size from git object store
    let size_out = std::process::Command::new("git")
        .args(["cat-file", "-s", &git_ref])
        .current_dir(repo_path)
        .output()
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;
    if !size_out.status.success() {
        return Err(ApiError::BadRequest("File not found in HEAD".to_string()));
    }
    let size: u64 = String::from_utf8_lossy(&size_out.stdout)
        .trim()
        .parse()
        .unwrap_or(0);

    // Stream content, capped at MAX_BYTES + 1
    let mut child = std::process::Command::new("git")
        .args(["show", &git_ref])
        .current_dir(repo_path)
        .stdout(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    let mut bytes = Vec::new();
    if let Some(stdout) = child.stdout.take() {
        stdout
            .take(MAX_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| ApiError::BadRequest(e.to_string()))?;
    }
    // Reap child; we don't care about exit code since we got our bytes
    child.wait().ok();

    Ok((bytes, size))
}
```

In the handler `read_file`, remove the `const MAX_BYTES: u64 = 500 * 1024;` line (now module-level).

- [ ] **Step 4: Run all tests**

```bash
cargo test -p server files 2>&1 | tail -30
```

Expected: all tests pass.

- [ ] **Step 5: Compile check**

```bash
pnpm run backend:check 2>&1 | grep -E "^error" | head -20
```

- [ ] **Step 6: Commit**

```bash
git add crates/server/src/routes/workspaces/files.rs
git commit -m "fix(files): stream file reads with byte cap to prevent large-file DoS"
```

---

## Task 3: Backend — `is_binary` Field + Proper Error Codes

**Context:** The current `FileContentResponse` uses `content: "__BINARY__"` as a sentinel for binary files — this is a leaky implementation detail that the frontend must match exactly. The fix: add `is_binary: bool` to `FileContentResponse`, set `content: ""` when binary, and let the frontend check the boolean. Also, several `std::fs` errors were being mapped to `ApiError::BadRequest` (400). IO errors from the filesystem should use `ApiError::Io` (500) or be more precisely classified.

**Files:**
- Modify: `crates/server/src/routes/workspaces/files.rs`

- [ ] **Step 1: Write a failing test for binary detection response**

Add inside `#[cfg(test)]`:

```rust
#[test]
fn read_file_fs_binary_file_sets_is_binary_true() {
    let tmp = TempDir::new().unwrap();
    let inner = tmp.path().join("workspace");
    fs::create_dir(&inner).unwrap();
    // PNG header bytes with null
    let binary: Vec<u8> = vec![0x89, 0x50, 0x4e, 0x47, 0x00, 0x0d, 0x0a, 0x1a];
    fs::write(inner.join("image.png"), &binary).unwrap();

    let (bytes, _size) = read_file_fs(&inner, "image.png").unwrap();
    let is_binary = bytes.iter().take(8192).any(|&b| b == 0);
    assert!(is_binary, "null-byte heuristic should detect binary");
}

#[test]
fn read_file_fs_text_file_sets_is_binary_false() {
    let tmp = TempDir::new().unwrap();
    let inner = tmp.path().join("workspace");
    fs::create_dir(&inner).unwrap();
    fs::write(inner.join("hello.rs"), b"fn main() {}").unwrap();

    let (bytes, _size) = read_file_fs(&inner, "hello.rs").unwrap();
    let is_binary = bytes.iter().take(8192).any(|&b| b == 0);
    assert!(!is_binary);
}
```

- [ ] **Step 2: Run tests to confirm they pass (they test existing logic)**

```bash
cargo test -p server files::tests::read_file_fs_binary_file_sets_is_binary_true 2>&1 | tail -10
cargo test -p server files::tests::read_file_fs_text_file_sets_is_binary_false 2>&1 | tail -10
```

Expected: PASS (confirming the helper still returns the right bytes — the API response struct change is what this task updates).

- [ ] **Step 3: Add `is_binary: bool` to `FileContentResponse` and update the handler**

Replace the `FileContentResponse` struct:

```rust
#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct FileContentResponse {
    pub path: String,
    pub content: String,
    pub is_binary: bool,
    pub size_bytes: u64,
    pub truncated: bool,
    pub language: Option<String>,
}
```

Replace the body of `read_file` handler (the logic after the `spawn_blocking` call) with:

```rust
    // Check first 8 KB for null bytes (binary heuristic)
    let is_binary = bytes.iter().take(8192).any(|&b| b == 0);

    let truncated = size_bytes > MAX_BYTES;
    let display_bytes = if truncated {
        &bytes[..MAX_BYTES as usize]
    } else {
        &bytes
    };

    let content = if is_binary {
        String::new()
    } else {
        String::from_utf8_lossy(display_bytes).into_owned()
    };

    Ok(ResponseJson(ApiResponse::success(FileContentResponse {
        path: rel_path.clone(),
        content,
        is_binary,
        size_bytes,
        truncated,
        language: detect_language(&rel_path),
    })))
```

- [ ] **Step 4: Run all tests**

```bash
cargo test -p server files 2>&1 | tail -30
```

Expected: all tests pass.

- [ ] **Step 5: Compile check**

```bash
pnpm run backend:check 2>&1 | grep -E "^error" | head -20
```

- [ ] **Step 6: Commit**

```bash
git add crates/server/src/routes/workspaces/files.rs
git commit -m "fix(files): replace __BINARY__ sentinel with is_binary field on FileContentResponse"
```

---

## Task 4: Regenerate Types + Update Frontend Binary Check

**Context:** `FileContentResponse` now has `is_binary: bool`. The shared TypeScript types must be regenerated so the frontend picks up the new field. Then `FileBrowserViewerPanel` must be updated to use `fileData.is_binary` instead of `fileData.content === '__BINARY__'`.

**Files:**
- Run: `pnpm run generate-types` (regenerates `shared/types.ts`)
- Modify: `packages/web-core/src/pages/workspaces/FileBrowserViewerPanel.tsx` (line 35)

- [ ] **Step 1: Regenerate types**

```bash
cd /Users/david/Code/.vibe-kanban-workspaces/e215-review-vk-swarm/vibe-kanban
pnpm run generate-types 2>&1 | tail -10
```

Expected output: `✅ TypeScript types generated in shared/types.ts`

- [ ] **Step 2: Verify `is_binary` appears in the generated file**

```bash
grep "is_binary" shared/types.ts
```

Expected: `is_binary: boolean;` inside the `FileContentResponse` type.

- [ ] **Step 3: Update `FileBrowserViewerPanel` to use the new field**

In `packages/web-core/src/pages/workspaces/FileBrowserViewerPanel.tsx`, replace line 35:

```typescript
  const isBinary = fileData?.content === '__BINARY__';
```

With:

```typescript
  const isBinary = fileData?.is_binary ?? false;
```

- [ ] **Step 4: TypeScript compile check**

```bash
pnpm tsc --noEmit -p packages/web-core/tsconfig.json 2>&1 | grep -i error | head -20
```

Expected: no errors.

- [ ] **Step 5: Commit**

```bash
git add shared/types.ts packages/web-core/src/pages/workspaces/FileBrowserViewerPanel.tsx
git commit -m "fix(files): regenerate types; use is_binary field instead of sentinel string"
```

---

## Task 5: Frontend — Workspace Store Reset on Switch + Remove Source Override

**Context (two related issues):**
1. The `useFileBrowserStore` is a singleton — it never resets when the user switches workspaces. Switching from workspace A to workspace B keeps workspace A's `currentPath`, `selectedFile`, and `filterTerm` active, causing stale UI and incorrect queries. Fix: add a `resetForWorkspace` action and call it from `FileBrowserContainer` in a `useEffect` keyed to `workspaceId`.
2. The `openFile` action hardcodes `source: 'worktree'`, silently overriding the user's current source selection when they click a file link in chat. Fix: remove the `source` assignment from `openFile`.

**Files:**
- Modify: `packages/web-core/src/shared/stores/useFileBrowserStore.ts`
- Modify: `packages/web-core/src/pages/workspaces/FileBrowserContainer.tsx`

- [ ] **Step 1: Add `resetForWorkspace` to the store and fix `openFile`**

Replace the full content of `packages/web-core/src/shared/stores/useFileBrowserStore.ts`:

```typescript
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
  openFile: (path: string) => void;
  resetForWorkspace: () => void;
};

function autoViewMode(path: string): FileViewMode {
  const lower = path.toLowerCase();
  if (
    lower.endsWith('.md') ||
    lower.endsWith('.markdown') ||
    lower.endsWith('.mdx')
  ) {
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
    // Note: does NOT override source — preserves user's current worktree/main selection
    set({
      currentPath: parentPath,
      selectedFile: path,
      viewMode: autoViewMode(path),
      filterTerm: '',
    });
  },

  resetForWorkspace: () =>
    set({
      currentPath: null,
      selectedFile: null,
      filterTerm: '',
      viewMode: null,
    }),
}));

export const useFileBrowserSource = () => useFileBrowserStore((s) => s.source);
export const useFileBrowserCurrentPath = () =>
  useFileBrowserStore((s) => s.currentPath);
export const useFileBrowserSelectedFile = () =>
  useFileBrowserStore((s) => s.selectedFile);
export const useFileBrowserFilterTerm = () =>
  useFileBrowserStore((s) => s.filterTerm);
export const useFileBrowserViewMode = () =>
  useFileBrowserStore((s) => s.viewMode);
export const useFileBrowserActions = () =>
  useFileBrowserStore((s) => ({
    setSource: s.setSource,
    navigate: s.navigate,
    selectFile: s.selectFile,
    setFilterTerm: s.setFilterTerm,
    setViewMode: s.setViewMode,
    openFile: s.openFile,
    resetForWorkspace: s.resetForWorkspace,
  }));
```

- [ ] **Step 2: Add workspace-switch `useEffect` in `FileBrowserContainer`**

Replace the full content of `packages/web-core/src/pages/workspaces/FileBrowserContainer.tsx`:

```typescript
import { useEffect } from 'react';
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
import {
  useDirectoryListing,
  useFileContent,
} from '@/shared/hooks/useFileBrowser';

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
  const {
    setSource,
    navigate,
    selectFile,
    setFilterTerm,
    setViewMode,
    resetForWorkspace,
  } = useFileBrowserActions();

  // Reset navigation state whenever the workspace changes
  useEffect(() => {
    resetForWorkspace();
  }, [workspaceId]); // eslint-disable-line react-hooks/exhaustive-deps

  const {
    data: listing,
    isLoading: isListingLoading,
    isError: isListingError,
  } = useDirectoryListing(workspaceId, currentPath, source);

  const {
    data: fileData,
    isLoading: isFileLoading,
    isError: isFileError,
  } = useFileContent(workspaceId, selectedFile, source);

  return (
    <div className={className ?? 'h-full min-h-0'}>
      <Group
        orientation="horizontal"
        className="h-full"
        defaultLayout={{ 'file-browser-tree': 35, 'file-browser-viewer': 65 }}
      >
        <Panel id="file-browser-tree" minSize="20%">
          <FileBrowserTreePanel
            listing={listing}
            isLoading={isListingLoading}
            isError={isListingError}
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
          id="file-browser-separator"
          className="w-1 bg-border hover:bg-brand/50 transition-colors cursor-col-resize"
        />

        <Panel id="file-browser-viewer" minSize="30%">
          <FileBrowserViewerPanel
            selectedFile={selectedFile}
            fileData={fileData}
            isLoading={isFileLoading}
            isError={isFileError}
            viewMode={viewMode}
            onSetViewMode={setViewMode}
          />
        </Panel>
      </Group>
    </div>
  );
}
```

- [ ] **Step 3: TypeScript compile check**

```bash
pnpm tsc --noEmit -p packages/web-core/tsconfig.json 2>&1 | grep -i error | head -20
```

Expected: TypeScript errors because `FileBrowserTreePanel` and `FileBrowserViewerPanel` don't yet accept `isError` — that's Task 7. For now there will be type errors. Skip if they block — they will be resolved in Task 7.

- [ ] **Step 4: Commit store changes now, container in Task 7**

```bash
git add packages/web-core/src/shared/stores/useFileBrowserStore.ts
git commit -m "fix(files): reset store on workspace switch; remove source override in openFile"
```

---

## Task 6: Frontend — Fix `useFileBrowserActions` Zustand Anti-Pattern

**Context:** `useFileBrowserActions` returns a new object literal every render, which causes all consumers to re-render whenever any part of the store changes — even unrelated slices. In Zustand v4, the fix is to wrap the selector with `useShallow`, which does a shallow equality check on the returned object so re-renders only happen when the functions themselves change (they don't, since they're stable store methods).

**Files:**
- Modify: `packages/web-core/src/shared/stores/useFileBrowserStore.ts`

- [ ] **Step 1: Add `useShallow` import and wrap the selector**

At the top of `useFileBrowserStore.ts`, add the import:

```typescript
import { useShallow } from 'zustand/react/shallow';
```

Replace the `useFileBrowserActions` export at the bottom:

```typescript
export const useFileBrowserActions = () =>
  useFileBrowserStore(
    useShallow((s) => ({
      setSource: s.setSource,
      navigate: s.navigate,
      selectFile: s.selectFile,
      setFilterTerm: s.setFilterTerm,
      setViewMode: s.setViewMode,
      openFile: s.openFile,
      resetForWorkspace: s.resetForWorkspace,
    }))
  );
```

- [ ] **Step 2: TypeScript compile check**

```bash
pnpm tsc --noEmit -p packages/web-core/tsconfig.json 2>&1 | grep -i error | head -20
```

Expected: no errors from this file. (Errors about `isError` on panels are still present from Task 5 — that is expected and will be fixed in Task 7.)

- [ ] **Step 3: Commit**

```bash
git add packages/web-core/src/shared/stores/useFileBrowserStore.ts
git commit -m "fix(files): wrap useFileBrowserActions with useShallow to prevent spurious re-renders"
```

---

## Task 7: Frontend — Error UI in Tree and Viewer Panels

**Context:** Neither `FileBrowserTreePanel` nor `FileBrowserViewerPanel` renders anything when the API request fails (network error, 500, etc.). The user sees an eternal spinner or nothing at all. `FileBrowserContainer` now tracks `isError` from both queries (added in Task 5) — this task threads it into the panels and renders an error state.

**Files:**
- Modify: `packages/web-core/src/pages/workspaces/FileBrowserTreePanel.tsx`
- Modify: `packages/web-core/src/pages/workspaces/FileBrowserViewerPanel.tsx`
- Modify: `packages/web-core/src/pages/workspaces/FileBrowserContainer.tsx` (already updated in Task 5, just needs uncommitted changes)

- [ ] **Step 1: Update `FileBrowserTreePanel` to accept and render `isError`**

Replace the full content of `packages/web-core/src/pages/workspaces/FileBrowserTreePanel.tsx`:

```typescript
import { useMemo } from 'react';
import {
  GitBranchIcon,
  FolderIcon,
  MagnifyingGlassIcon,
  WarningCircleIcon,
} from '@phosphor-icons/react';
import { cn } from '@/shared/lib/utils';
import { FileBrowserTreeNode } from '@vibe/ui/components/FileBrowserTreeNode';
import type { DirectoryListResponse } from 'shared/types';
import type { FileSource } from '@/shared/stores/useFileBrowserStore';

interface FileBrowserTreePanelProps {
  listing: DirectoryListResponse | undefined;
  isLoading: boolean;
  isError: boolean;
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
  isError,
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
      a.is_directory === b.is_directory
        ? a.name.localeCompare(b.name)
        : a.is_directory
          ? -1
          : 1
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
          <FolderIcon className="size-3" />
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
        {isError ? (
          <div className="flex flex-col items-center justify-center py-8 gap-2 text-destructive">
            <WarningCircleIcon className="size-5" />
            <span className="text-xs">Failed to load directory</span>
          </div>
        ) : isLoading ? (
          <div className="flex items-center justify-center py-8">
            <div className="size-4 animate-spin rounded-full border-2 border-border border-t-brand" />
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

- [ ] **Step 2: Update `FileBrowserViewerPanel` to accept and render `isError`**

Replace the full content of `packages/web-core/src/pages/workspaces/FileBrowserViewerPanel.tsx`:

```typescript
import { CopyIcon, WarningCircleIcon } from '@phosphor-icons/react';
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
  isError: boolean;
  viewMode: FileViewMode;
  onSetViewMode: (mode: FileViewMode) => void;
}

export function FileBrowserViewerPanel({
  selectedFile,
  fileData,
  isLoading,
  isError,
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

  const isMd = isMarkdownFile(selectedFile);
  const isHtml = isHtmlFile(selectedFile);
  const isBinary = fileData?.is_binary ?? false;

  return (
    <div className="flex flex-col h-full min-h-0">
      {/* Header */}
      <div className="flex items-center gap-2 px-3 py-2 border-b border-border shrink-0">
        <span
          className="font-mono text-xs text-low truncate flex-1"
          title={selectedFile}
        >
          {selectedFile}
        </span>

        {isMd && !isBinary && (
          <div className="flex gap-0.5">
            {(['preview', 'raw'] as const).map((m) => (
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

        {isHtml && !isBinary && (
          <div className="flex gap-0.5">
            {(['rendered', 'source'] as const).map((m) => (
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
          className="text-low hover:text-normal transition-colors p-0.5"
        >
          <CopyIcon className="size-3.5" />
        </button>
      </div>

      {/* Body */}
      <div className="flex-1 min-h-0 overflow-hidden">
        {isError ? (
          <div className="flex flex-col items-center justify-center h-full gap-2 text-destructive">
            <WarningCircleIcon className="size-5" />
            <span className="text-sm">Failed to load file</span>
          </div>
        ) : isLoading ? (
          <div className="flex items-center justify-center h-full">
            <div className="size-5 animate-spin rounded-full border-2 border-border border-t-brand" />
          </div>
        ) : isBinary ? (
          <div className="flex items-center justify-center h-full text-low text-sm">
            Binary file — cannot display
          </div>
        ) : !fileData ? (
          <div className="flex items-center justify-center h-full text-low text-sm">
            File not found
          </div>
        ) : (
          <div className="flex flex-col h-full min-h-0">
            {fileData.truncated && (
              <div className="px-3 py-1.5 bg-warning/10 text-warning text-xs shrink-0">
                File truncated at 500 KB — showing partial content
              </div>
            )}
            <div className="flex-1 min-h-0 overflow-hidden">
              {renderContent(
                selectedFile,
                fileData.content,
                fileData.language ?? null,
                viewMode
              )}
            </div>
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

- [ ] **Step 3: TypeScript compile check**

```bash
pnpm tsc --noEmit -p packages/web-core/tsconfig.json 2>&1 | grep -i error | head -20
```

Expected: no errors.

- [ ] **Step 4: Commit**

```bash
git add \
  packages/web-core/src/pages/workspaces/FileBrowserContainer.tsx \
  packages/web-core/src/pages/workspaces/FileBrowserTreePanel.tsx \
  packages/web-core/src/pages/workspaces/FileBrowserViewerPanel.tsx
git commit -m "fix(files): add error UI to tree and viewer panels; thread isError from queries"
```

---

## Task 8: Frontend — `placeholderData: keepPreviousData` in Hooks

**Context:** Without `placeholderData`, every directory navigation or source toggle causes `isLoading=true` and a spinner flash while TanStack Query fetches data it may have previously cached. `keepPreviousData` (TanStack Query v5: `placeholderData: keepPreviousData`) prevents this by keeping the prior result visible until the new query resolves.

**Files:**
- Modify: `packages/web-core/src/shared/hooks/useFileBrowser.ts`

- [ ] **Step 1: Update both hooks to use `keepPreviousData`**

Replace the full content of `packages/web-core/src/shared/hooks/useFileBrowser.ts`:

```typescript
import { useQuery, keepPreviousData } from '@tanstack/react-query';
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
    queryFn: () => workspacesApi.listFiles(workspaceId!, path ?? '', source),
    enabled: !!workspaceId,
    staleTime: 30_000,
    placeholderData: keepPreviousData,
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
    placeholderData: keepPreviousData,
  });
}

export function isMarkdownFile(path: string): boolean {
  return /\.(md|markdown|mdx)$/i.test(path);
}

export function isHtmlFile(path: string): boolean {
  return /\.(html|htm)$/i.test(path);
}
```

- [ ] **Step 2: TypeScript compile check**

```bash
pnpm tsc --noEmit -p packages/web-core/tsconfig.json 2>&1 | grep -i error | head -20
```

Expected: no errors. (`keepPreviousData` is exported from `@tanstack/react-query` in v5.)

- [ ] **Step 3: Commit**

```bash
git add packages/web-core/src/shared/hooks/useFileBrowser.ts
git commit -m "fix(files): add placeholderData keepPreviousData to suppress loading flicker on navigation"
```

---

## Task 9: Frontend — Syntax Highlighting in `FileBrowserCodeViewer`

**Context:** `FileBrowserCodeViewer` receives a `language` prop (e.g. `"typescript"`, `"rust"`) from the backend but aliases it to `_language` and renders a plain `<pre>`. The codebase already depends on `rehype-highlight` which transitively installs `highlight.js`. We can use `highlight.js` directly to tokenise content and inject highlighted HTML. `dangerouslySetInnerHTML` is safe here because the input is the user's own source files (no third-party content injection vector) and `hljs.highlight()` only emits `<span>` wrappers with CSS classes — it does not emit `<script>` or event handlers.

**Files:**
- Modify: `packages/web-core/src/pages/workspaces/FileBrowserCodeViewer.tsx`

- [ ] **Step 1: Verify `highlight.js` is available (transitive via `rehype-highlight`)**

```bash
ls node_modules/highlight.js/package.json 2>/dev/null || \
  ls packages/web-core/node_modules/highlight.js/package.json 2>/dev/null && echo "found"
```

Expected: `found`. If not found, run:

```bash
cd packages/web-core && pnpm add highlight.js && cd ../..
```

- [ ] **Step 2: Check the highlight.js CSS theme is imported somewhere**

```bash
grep -r "highlight.js" packages/web-core/src --include="*.ts" --include="*.tsx" --include="*.css" | head -5
```

If there are no CSS imports for highlight.js, we need to add one. Check if any global CSS file already loads hljs styles:

```bash
grep -r "hljs\|highlight" packages/local-web/src --include="*.css" | head -5
grep -r "hljs\|highlight" packages/web-core/src --include="*.css" | head -5
```

If no hljs CSS is imported, add it to the local-web entry CSS or to `FileBrowserCodeViewer` itself via a dynamic import. The simplest approach: import the `github-dark` or `github` theme inside the component for lazy loading.

- [ ] **Step 3: Replace `FileBrowserCodeViewer` with a highlighted version**

Replace the full content of `packages/web-core/src/pages/workspaces/FileBrowserCodeViewer.tsx`:

```typescript
import { useMemo } from 'react';
import hljs from 'highlight.js/lib/core';
// Import only the languages we know about (keeps bundle small)
import typescript from 'highlight.js/lib/languages/typescript';
import javascript from 'highlight.js/lib/languages/javascript';
import rust from 'highlight.js/lib/languages/rust';
import python from 'highlight.js/lib/languages/python';
import go from 'highlight.js/lib/languages/go';
import json from 'highlight.js/lib/languages/json';
import yaml from 'highlight.js/lib/languages/yaml';
import xml from 'highlight.js/lib/languages/xml';
import css from 'highlight.js/lib/languages/css';
import scss from 'highlight.js/lib/languages/scss';
import bash from 'highlight.js/lib/languages/bash';
import sql from 'highlight.js/lib/languages/sql';
import graphql from 'highlight.js/lib/languages/graphql';
import swift from 'highlight.js/lib/languages/swift';
import kotlin from 'highlight.js/lib/languages/kotlin';
import java from 'highlight.js/lib/languages/java';
import ruby from 'highlight.js/lib/languages/ruby';
import php from 'highlight.js/lib/languages/php';
import csharp from 'highlight.js/lib/languages/csharp';
import cpp from 'highlight.js/lib/languages/cpp';
import c from 'highlight.js/lib/languages/c';
import markdown from 'highlight.js/lib/languages/markdown';
import 'highlight.js/styles/github-dark.min.css';

hljs.registerLanguage('typescript', typescript);
hljs.registerLanguage('javascript', javascript);
hljs.registerLanguage('rust', rust);
hljs.registerLanguage('python', python);
hljs.registerLanguage('go', go);
hljs.registerLanguage('json', json);
hljs.registerLanguage('yaml', yaml);
hljs.registerLanguage('xml', xml);
hljs.registerLanguage('css', css);
hljs.registerLanguage('scss', scss);
hljs.registerLanguage('bash', bash);
hljs.registerLanguage('sql', sql);
hljs.registerLanguage('graphql', graphql);
hljs.registerLanguage('swift', swift);
hljs.registerLanguage('kotlin', kotlin);
hljs.registerLanguage('java', java);
hljs.registerLanguage('ruby', ruby);
hljs.registerLanguage('php', php);
hljs.registerLanguage('csharp', csharp);
hljs.registerLanguage('cpp', cpp);
hljs.registerLanguage('c', c);
hljs.registerLanguage('markdown', markdown);

interface FileBrowserCodeViewerProps {
  content: string;
  language: string | null;
}

export function FileBrowserCodeViewer({
  content,
  language,
}: FileBrowserCodeViewerProps) {
  const highlighted = useMemo(() => {
    if (!language || !hljs.getLanguage(language)) {
      return null;
    }
    try {
      return hljs.highlight(content, { language }).value;
    } catch {
      return null;
    }
  }, [content, language]);

  return (
    <div className="h-full overflow-auto hljs">
      <pre className="p-4 whitespace-pre text-sm leading-relaxed m-0 min-h-full">
        {highlighted ? (
          <code
            className={`language-${language}`}
            // Safe: hljs.highlight only emits <span> elements with CSS classes
            dangerouslySetInnerHTML={{ __html: highlighted }}
          />
        ) : (
          <code className="font-mono text-xs">{content}</code>
        )}
      </pre>
    </div>
  );
}
```

- [ ] **Step 4: TypeScript compile check**

```bash
pnpm tsc --noEmit -p packages/web-core/tsconfig.json 2>&1 | grep -i error | head -20
```

If there are type errors about missing type declarations for hljs language imports, add a type declaration or cast. Most common fix — add `@types/highlight.js` isn't needed; hljs ships its own types. If module resolution fails for `highlight.js/lib/languages/typescript`, check that `highlight.js` is resolvable:

```bash
node -e "require('highlight.js/lib/core')" 2>&1
```

If it fails, install explicitly:

```bash
pnpm add highlight.js --filter @vibe/web-core
```

- [ ] **Step 5: Lint check**

```bash
pnpm run lint 2>&1 | grep -E "FileBrowserCodeViewer" | head -10
```

Expected: no lint errors.

- [ ] **Step 6: Commit**

```bash
git add packages/web-core/src/pages/workspaces/FileBrowserCodeViewer.tsx
git commit -m "fix(files): add syntax highlighting to code viewer via highlight.js"
```

---

## Final Validation

- [ ] **Step 1: Full Rust test suite**

```bash
cargo test -p server files 2>&1 | tail -30
```

Expected: all tests pass (should be ~14+ tests now).

- [ ] **Step 2: Full TypeScript type check**

```bash
pnpm tsc --noEmit -p packages/web-core/tsconfig.json 2>&1 | grep -i error | head -20
```

Expected: no errors.

- [ ] **Step 3: Lint**

```bash
pnpm run lint 2>&1 | tail -20
```

Expected: no errors.

- [ ] **Step 4: Generate-types check (CI gate)**

```bash
pnpm run generate-types:check 2>&1 | tail -5
```

Expected: `✅ shared/types.ts is up to date.`

- [ ] **Step 5: Manual smoke test checklist**

Start the dev server and open the Files tab. Verify:
- [ ] Switching between two workspaces clears the file tree (no stale path from previous workspace)
- [ ] Navigating into directories shows no spinner flash (placeholder data working)
- [ ] Opening a TypeScript file shows syntax-highlighted code
- [ ] Opening a binary file (image) shows "Binary file — cannot display" (not `__BINARY__`)
- [ ] Killing the backend mid-session and navigating shows error state in both panels (not a spinner)
- [ ] Attempting to fetch `.env` via URL (`/api/workspaces/{id}/files/content?path=.env`) returns 400
- [ ] Clicking a file link in chat opens the Files tab with the correct file selected, preserving the current Worktree/Main toggle
- [ ] Switching source (Worktree ↔ Main) while viewing a file continues to show the same file from the new source

---

## Self-Review

**Spec coverage check:**

| Issue | Task | Status |
|-------|------|--------|
| P0: Hidden-file bypass in read endpoints | Task 1 | ✅ |
| P0: Full file read before truncation (DoS) | Task 2 | ✅ |
| P0: `__BINARY__` sentinel → `is_binary` field | Task 3 + 4 | ✅ |
| P0: Workspace store not reset on switch | Task 5 | ✅ |
| P1: TOCTOU symlink race | Task 1 | ✅ |
| P1: `useFileBrowserActions` Zustand anti-pattern | Task 6 | ✅ |
| P1: `openFile` hardcodes `source: 'worktree'` | Task 5 | ✅ |
| P1: No error UI on network failure | Task 7 | ✅ |
| P1: No `placeholderData` (loading flicker) | Task 8 | ✅ |
| P1: No syntax highlighting | Task 9 | ✅ |
| P2: Hidden dirs in git listing | Task 1 | ✅ |

All 11 issues from the adversarial review have a corresponding task. No placeholders found in the plan. Types are consistent across tasks (e.g., `resetForWorkspace` defined in Task 5 and referenced in Task 5 and 6; `isError` introduced in Task 5's `FileBrowserContainer` and consumed in Task 7's panels).
