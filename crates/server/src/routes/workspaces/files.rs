use std::{io::Read, path::Path};

use axum::{
    Extension, Router,
    extract::{Query, State},
    response::Json as ResponseJson,
    routing::get,
};
use db::models::{workspace::Workspace, workspace_repo::WorkspaceRepo};
use deployment::Deployment;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use utils::response::ApiResponse;

use crate::{DeploymentImpl, error::ApiError};

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
    pub is_binary: bool,
    pub size_bytes: u64,
    pub truncated: bool,
    pub language: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum FileSource {
    #[default]
    Worktree,
    Main,
}

#[derive(Debug, Deserialize)]
pub struct FilesQuery {
    pub path: Option<String>,
    #[serde(default)]
    pub source: FileSource,
}

const MAX_BYTES: u64 = 500 * 1024;

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
    let rel_path = query.path.as_deref().unwrap_or("").to_string();
    let repo_path_owned = repo.path.clone();
    let source = query.source;

    let entries = tokio::task::spawn_blocking(move || match source {
        FileSource::Main => list_directory_git(&repo_path_owned, &rel_path),
        FileSource::Worktree => list_directory_fs(&worktree_root, &rel_path),
    })
    .await
    .map_err(|e| ApiError::BadRequest(e.to_string()))??;

    let current_path = query.path.unwrap_or_default();

    Ok(ResponseJson(ApiResponse::success(DirectoryListResponse {
        entries,
        current_path,
    })))
}

fn list_directory_fs(
    worktree_root: &Path,
    rel_path: &str,
) -> Result<Vec<DirectoryEntry>, ApiError> {
    // Reject paths with any hidden component
    if rel_path
        .split('/')
        .any(|c| !c.is_empty() && c.starts_with('.'))
    {
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

fn list_directory_git(repo_path: &Path, rel_path: &str) -> Result<Vec<DirectoryEntry>, ApiError> {
    if rel_path.contains("..") || rel_path.starts_with('/') || rel_path.starts_with('-') {
        return Err(ApiError::BadRequest("Invalid path".to_string()));
    }
    if rel_path
        .split('/')
        .any(|c| !c.is_empty() && c.starts_with('.'))
    {
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
        .ok_or_else(|| ApiError::BadRequest("path query param required".to_string()))?;
    let repo_path_owned = repo.path.clone();
    let source = query.source;

    let rel_path_owned = rel_path.clone();
    let (bytes, size_bytes) = tokio::task::spawn_blocking(move || match source {
        FileSource::Main => read_file_git(&repo_path_owned, &rel_path_owned),
        FileSource::Worktree => read_file_fs(&worktree_root, &rel_path_owned),
    })
    .await
    .map_err(|e| ApiError::BadRequest(e.to_string()))??;

    // Check first 8KB for null bytes (binary heuristic)
    let is_binary = bytes.iter().take(8192).any(|&b: &u8| b == 0);

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
}

fn read_file_fs(worktree_root: &Path, rel_path: &str) -> Result<(Vec<u8>, u64), ApiError> {
    // Reject hidden path components
    if rel_path
        .split('/')
        .any(|c| !c.is_empty() && c.starts_with('.'))
    {
        return Err(ApiError::BadRequest(
            "Hidden files are not accessible".to_string(),
        ));
    }
    let canonical_root = worktree_root
        .canonicalize()
        .map_err(|_| ApiError::BadRequest("Workspace root not found".to_string()))?;
    // Symlink guard: walk each component and reject any symlink in the chain
    let mut accumulated = canonical_root.clone();
    for component in std::path::Path::new(rel_path).components() {
        accumulated = accumulated.join(component);
        match std::fs::symlink_metadata(&accumulated) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err(ApiError::BadRequest(
                    "Symlink access not allowed".to_string(),
                ));
            }
            Ok(_) => {}
            Err(_) => {
                return Err(ApiError::BadRequest("File not found".to_string()));
            }
        }
    }
    let target = canonical_root.join(rel_path);
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
    // Stat for true file size, then stream limited bytes
    let size = std::fs::metadata(&canonical)
        .map_err(|e| ApiError::BadRequest(e.to_string()))?
        .len();
    let mut file =
        std::fs::File::open(&canonical).map_err(|e| ApiError::BadRequest(e.to_string()))?;
    let mut bytes = Vec::new();
    file.by_ref()
        .take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;
    Ok((bytes, size))
}

fn read_file_git(repo_path: &Path, rel_path: &str) -> Result<(Vec<u8>, u64), ApiError> {
    if rel_path.contains("..") || rel_path.starts_with('/') || rel_path.starts_with('-') {
        return Err(ApiError::BadRequest("Invalid path".to_string()));
    }
    if rel_path
        .split('/')
        .any(|c| !c.is_empty() && c.starts_with('.'))
    {
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
        .map_err(|e| ApiError::BadRequest(format!("git cat-file size: {e}")))?;

    // Stream content capped at MAX_BYTES + 1
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
    // Kill the child explicitly so truncated large reads don't leave git
    // blocking on a full pipe buffer waiting for SIGPIPE.
    let _ = child.kill();
    let _ = child.wait();

    Ok((bytes, size))
}

pub fn router() -> Router<DeploymentImpl> {
    Router::new()
        .route("/", get(list_directory))
        .route("/content", get(read_file))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;

    #[test]
    fn list_directory_fs_sorts_dirs_first_then_alpha() {
        let base = TempDir::new().unwrap();
        fs::write(base.path().join("alpha.ts"), "export {}").unwrap();
        fs::write(base.path().join("beta.md"), "# hello").unwrap();
        fs::create_dir(base.path().join("src")).unwrap();
        fs::write(base.path().join("src").join("main.rs"), "fn main(){}").unwrap();

        let entries = list_directory_fs(base.path(), "").unwrap();
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names[0], "src");
        assert!(names[1] < names[2]);
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

    #[test]
    fn read_file_fs_binary_file_returns_is_binary_true_empty_content() {
        // Demonstrates that the binary heuristic sets is_binary=true and content=""
        // (We test the byte-level heuristic directly since it's used in the handler)
        let binary_bytes: Vec<u8> = vec![0x89, 0x50, 0x4e, 0x47, 0x00, 0x0d, 0x0a, 0x1a];
        let is_binary = binary_bytes.iter().take(8192).any(|&b| b == 0);
        assert!(
            is_binary,
            "PNG bytes with null should be detected as binary"
        );

        // When is_binary=true, content must be empty (not the __BINARY__ sentinel)
        let content = if is_binary {
            String::new()
        } else {
            String::from_utf8_lossy(&binary_bytes).into_owned()
        };
        assert_eq!(
            content, "",
            "binary content should be empty string, not a sentinel"
        );
    }

    #[test]
    fn binary_detection_null_byte_heuristic() {
        // File with null byte should be detected as binary
        let binary_bytes: Vec<u8> = vec![0x89, 0x50, 0x4e, 0x47, 0x00, 0x0d, 0x0a, 0x1a];
        let is_binary = binary_bytes.iter().take(8192).any(|&b| b == 0);
        assert!(is_binary);

        // Plain text should not be detected as binary
        let text_bytes = b"fn main() { println!(\"hello\"); }".to_vec();
        let is_binary = text_bytes.iter().take(8192).any(|&b| b == 0);
        assert!(!is_binary);
    }

    #[test]
    fn read_file_fs_rejects_hidden_file() {
        let tmp = TempDir::new().unwrap();
        let inner = tmp.path().join("workspace");
        fs::create_dir(&inner).unwrap();
        fs::write(inner.join(".env"), "SECRET=abc").unwrap();
        let result = read_file_fs(&inner, ".env");
        assert!(result.is_err());
        let msg = format!("{:?}", result.unwrap_err());
        assert!(
            msg.contains("Hidden") || msg.contains("hidden"),
            "err was: {msg}"
        );
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
        let tmp = TempDir::new().unwrap();
        let result = list_directory_git(tmp.path(), ".git");
        assert!(result.is_err());
        let msg = format!("{:?}", result.unwrap_err());
        assert!(
            msg.contains("Hidden") || msg.contains("hidden"),
            "expected hidden-path error, got: {msg}"
        );
    }

    #[test]
    fn read_file_git_rejects_hidden_file() {
        let tmp = TempDir::new().unwrap();
        let result = read_file_git(tmp.path(), ".env");
        assert!(result.is_err());
        let msg = format!("{:?}", result.unwrap_err());
        assert!(
            msg.contains("Hidden") || msg.contains("hidden"),
            "expected hidden-file error, got: {msg}"
        );
    }

    #[test]
    fn read_file_fs_does_not_load_full_large_file() {
        let tmp = TempDir::new().unwrap();
        let inner = tmp.path().join("workspace");
        fs::create_dir(&inner).unwrap();
        // 600 KB file — larger than MAX_BYTES (500 KB)
        let big = vec![b'a'; 600 * 1024];
        fs::write(inner.join("big.txt"), &big).unwrap();

        let (bytes, size) = read_file_fs(&inner, "big.txt").unwrap();
        assert_eq!(size, 600 * 1024, "size should be full file size from stat");
        // With streaming, bytes read should be exactly MAX_BYTES + 1 = 512001
        assert_eq!(
            bytes.len(),
            500 * 1024 + 1,
            "read bytes should be exactly MAX_BYTES+1, got {}",
            bytes.len()
        );
    }

    #[test]
    #[cfg(unix)]
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
        assert!(
            msg.contains("ymlink") || msg.contains("not allowed"),
            "err was: {msg}"
        );
    }
}
