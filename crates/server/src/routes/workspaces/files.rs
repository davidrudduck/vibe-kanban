use axum::{
    Extension,
    Router,
    extract::{Query, State},
    response::Json as ResponseJson,
    routing::get,
};
use db::models::workspace::Workspace;
use db::models::workspace_repo::WorkspaceRepo;
use serde::{Deserialize, Serialize};
use std::path::Path;
use ts_rs::TS;
use utils::response::ApiResponse;

use deployment::Deployment;

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

    Ok(ResponseJson(ApiResponse::success(DirectoryListResponse {
        entries,
        current_path: rel_path.to_string(),
    })))
}

fn list_directory_fs(
    worktree_root: &Path,
    rel_path: &str,
) -> Result<Vec<DirectoryEntry>, ApiError> {
    let canonical_root = worktree_root
        .canonicalize()
        .map_err(|_| ApiError::BadRequest("Workspace root not found".to_string()))?;
    let target = canonical_root.join(rel_path);
    let canonical = target
        .canonicalize()
        .map_err(|_| ApiError::BadRequest("Directory not found".to_string()))?;
    if !canonical.starts_with(&canonical_root) {
        return Err(ApiError::BadRequest("Path traversal not allowed".to_string()));
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
    let tree_path = if rel_path.is_empty() {
        String::new()
    } else {
        format!("{}/", rel_path)
    };

    let output = std::process::Command::new("git")
        .args(["ls-tree", "--long", "HEAD", &tree_path])
        .current_dir(repo_path)
        .output()
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    if !output.status.success() {
        return Err(ApiError::BadRequest(
            "Path not found in HEAD".to_string(),
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut entries: Vec<DirectoryEntry> = stdout
        .lines()
        .filter_map(|line| {
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

    const MAX_BYTES: u64 = 512 * 1024;

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

    let content = String::from_utf8(display_bytes.to_vec()).unwrap_or_else(|_| {
        "__BINARY__".to_string()
    });

    Ok(ResponseJson(ApiResponse::success(FileContentResponse {
        path: rel_path.to_string(),
        content,
        size_bytes,
        truncated,
        language: detect_language(rel_path),
    })))
}

fn read_file_fs(
    worktree_root: &Path,
    rel_path: &str,
) -> Result<(Vec<u8>, u64), ApiError> {
    let canonical_root = worktree_root
        .canonicalize()
        .map_err(|_| ApiError::BadRequest("Workspace root not found".to_string()))?;
    let target = canonical_root.join(rel_path);
    let canonical = target
        .canonicalize()
        .map_err(|_| ApiError::BadRequest("File not found".to_string()))?;
    if !canonical.starts_with(&canonical_root) {
        return Err(ApiError::BadRequest("Path traversal not allowed".to_string()));
    }
    if canonical.is_dir() {
        return Err(ApiError::BadRequest("Path is a directory".to_string()));
    }
    let bytes = std::fs::read(&canonical).map_err(|e| ApiError::BadRequest(e.to_string()))?;
    let size = bytes.len() as u64;
    Ok((bytes, size))
}

fn read_file_git(repo_path: &Path, rel_path: &str) -> Result<(Vec<u8>, u64), ApiError> {
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

pub fn router() -> Router<DeploymentImpl> {
    Router::new()
        .route("/", get(list_directory))
        .route("/content", get(read_file))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

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
}
