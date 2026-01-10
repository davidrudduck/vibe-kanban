//! File browser and search handlers for projects.
//!
//! This module contains handlers for file operations:
//! - list_project_files: List files in a project directory
//! - read_project_file: Read file content
//! - read_project_file_by_remote_id: Read file by remote project ID
//! - search_project_files: Search files in a project

use std::path::Path as StdPath;

use axum::{
    Extension,
    extract::{Path, Query, State},
    response::Json as ResponseJson,
};
use db::models::project::{Project, SearchMatchType, SearchResult};
use deployment::Deployment;
use ignore::WalkBuilder;
use services::services::{
    file_ranker::FileRanker,
    file_search_cache::{CacheError, SearchMode, SearchQuery},
    filesystem::{DirectoryListResponse, FileContentResponse, FilesystemError},
};
use utils::response::ApiResponse;
use uuid::Uuid;

use crate::{DeploymentImpl, error::ApiError, middleware::RemoteProjectContext, proxy::check_remote_proxy};

use super::super::types::ListProjectFilesQuery;

/// List files and directories within a project's git repository
pub async fn list_project_files(
    Extension(project): Extension<Project>,
    remote_ctx: Option<Extension<RemoteProjectContext>>,
    State(deployment): State<DeploymentImpl>,
    Query(query): Query<ListProjectFilesQuery>,
) -> Result<ResponseJson<ApiResponse<DirectoryListResponse>>, ApiError> {
    // Check if this is a remote project that should be proxied
    if let Some(proxy_info) = check_remote_proxy(remote_ctx.as_ref().map(|e| &e.0))? {
        tracing::debug!(
            node_id = %proxy_info.node_id,
            remote_project_id = %proxy_info.target_id,
            path = ?query.path,
            "Proxying list_project_files to remote node"
        );

        let path = match &query.path {
            Some(p) => format!(
                "/projects/by-remote-id/{}/files?path={}",
                proxy_info.target_id,
                urlencoding::encode(p)
            ),
            None => format!("/projects/by-remote-id/{}/files", proxy_info.target_id),
        };
        let response: ApiResponse<DirectoryListResponse> = deployment
            .node_proxy_client()
            .proxy_get(&proxy_info.node_url, &path, proxy_info.node_id)
            .await?;

        return Ok(ResponseJson(response));
    }

    // Local project - execute directly
    match deployment
        .filesystem()
        .list_directory_within(&project.git_repo_path, query.path.as_deref())
        .await
    {
        Ok(response) => Ok(ResponseJson(ApiResponse::success(response))),
        Err(FilesystemError::DirectoryDoesNotExist) => {
            Ok(ResponseJson(ApiResponse::error("Directory does not exist")))
        }
        Err(FilesystemError::PathIsNotDirectory) => {
            Ok(ResponseJson(ApiResponse::error("Path is not a directory")))
        }
        Err(FilesystemError::PathTraversalNotAllowed) => Ok(ResponseJson(ApiResponse::error(
            "Path traversal not allowed",
        ))),
        Err(FilesystemError::Io(e)) => {
            tracing::error!("Failed to list project directory: {}", e);
            Ok(ResponseJson(ApiResponse::error(&format!(
                "Failed to list directory: {}",
                e
            ))))
        }
        Err(e) => {
            tracing::error!("Unexpected error listing project: {}", e);
            Ok(ResponseJson(ApiResponse::error(&e.to_string())))
        }
    }
}

/// Read file content from a project's git repository
pub async fn read_project_file(
    Extension(project): Extension<Project>,
    remote_ctx: Option<Extension<RemoteProjectContext>>,
    State(deployment): State<DeploymentImpl>,
    Path((_project_id, file_path)): Path<(Uuid, String)>,
) -> Result<ResponseJson<ApiResponse<FileContentResponse>>, ApiError> {
    // Check if this is a remote project that should be proxied
    if let Some(proxy_info) = check_remote_proxy(remote_ctx.as_ref().map(|e| &e.0))? {
        tracing::debug!(
            node_id = %proxy_info.node_id,
            remote_project_id = %proxy_info.target_id,
            file_path = %file_path,
            "Proxying read_project_file to remote node"
        );

        let path = format!(
            "/projects/by-remote-id/{}/files/{}",
            proxy_info.target_id, file_path
        );
        let response: ApiResponse<FileContentResponse> = deployment
            .node_proxy_client()
            .proxy_get(&proxy_info.node_url, &path, proxy_info.node_id)
            .await?;

        return Ok(ResponseJson(response));
    }

    // Local project - execute directly
    match deployment
        .filesystem()
        .read_file_within(&project.git_repo_path, &file_path, None)
        .await
    {
        Ok(response) => Ok(ResponseJson(ApiResponse::success(response))),
        Err(FilesystemError::FileDoesNotExist) => {
            Ok(ResponseJson(ApiResponse::error("File does not exist")))
        }
        Err(FilesystemError::PathIsNotFile) => {
            Ok(ResponseJson(ApiResponse::error("Path is not a file")))
        }
        Err(FilesystemError::PathTraversalNotAllowed) => Ok(ResponseJson(ApiResponse::error(
            "Path traversal not allowed",
        ))),
        Err(FilesystemError::FileIsBinary) => Ok(ResponseJson(ApiResponse::error(
            "Cannot display binary file",
        ))),
        Err(FilesystemError::FileTooLarge {
            max_bytes,
            actual_bytes,
        }) => Ok(ResponseJson(ApiResponse::error(&format!(
            "File too large ({} bytes, max {} bytes)",
            actual_bytes, max_bytes
        )))),
        Err(FilesystemError::Io(e)) => {
            tracing::error!("Failed to read project file: {}", e);
            Ok(ResponseJson(ApiResponse::error(&format!(
                "Failed to read file: {}",
                e
            ))))
        }
        Err(e) => {
            tracing::error!("Unexpected error reading project file: {}", e);
            Ok(ResponseJson(ApiResponse::error(&e.to_string())))
        }
    }
}

/// Read a file from a project looked up by remote_project_id.
/// This is the by-remote-id variant of read_project_file.
pub async fn read_project_file_by_remote_id(
    Extension(project): Extension<Project>,
    State(deployment): State<DeploymentImpl>,
    Path((_remote_project_id, file_path)): Path<(Uuid, String)>,
) -> Result<ResponseJson<ApiResponse<FileContentResponse>>, ApiError> {
    match deployment
        .filesystem()
        .read_file_within(&project.git_repo_path, &file_path, None)
        .await
    {
        Ok(response) => Ok(ResponseJson(ApiResponse::success(response))),
        Err(FilesystemError::FileDoesNotExist) => {
            Ok(ResponseJson(ApiResponse::error("File does not exist")))
        }
        Err(FilesystemError::PathIsNotFile) => {
            Ok(ResponseJson(ApiResponse::error("Path is not a file")))
        }
        Err(FilesystemError::PathTraversalNotAllowed) => Ok(ResponseJson(ApiResponse::error(
            "Path traversal not allowed",
        ))),
        Err(FilesystemError::FileIsBinary) => Ok(ResponseJson(ApiResponse::error(
            "Cannot display binary file",
        ))),
        Err(FilesystemError::FileTooLarge {
            max_bytes,
            actual_bytes,
        }) => Ok(ResponseJson(ApiResponse::error(&format!(
            "File too large ({} bytes, max {} bytes)",
            actual_bytes, max_bytes
        )))),
        Err(FilesystemError::Io(e)) => {
            tracing::error!("Failed to read project file: {}", e);
            Ok(ResponseJson(ApiResponse::error(&format!(
                "Failed to read file: {}",
                e
            ))))
        }
        Err(e) => {
            tracing::error!("Unexpected error reading file: {}", e);
            Ok(ResponseJson(ApiResponse::error(&e.to_string())))
        }
    }
}

pub async fn search_project_files(
    State(deployment): State<DeploymentImpl>,
    Extension(project): Extension<Project>,
    remote_ctx: Option<Extension<RemoteProjectContext>>,
    Query(search_query): Query<SearchQuery>,
) -> Result<ResponseJson<ApiResponse<Vec<SearchResult>>>, ApiError> {
    let query = search_query.q.trim();
    let mode = search_query.mode.clone();

    if query.is_empty() {
        return Ok(ResponseJson(ApiResponse::error(
            "Query parameter 'q' is required and cannot be empty",
        )));
    }

    // Check if this is a remote project that should be proxied
    if let Some(proxy_info) = check_remote_proxy(remote_ctx.as_ref().map(|e| &e.0))? {
        tracing::debug!(
            node_id = %proxy_info.node_id,
            remote_project_id = %proxy_info.target_id,
            query = %query,
            "Proxying search_project_files to remote node"
        );

        // Build query string
        let mode_str = match &search_query.mode {
            SearchMode::Settings => "settings",
            SearchMode::TaskForm => "task_form",
        };
        let path = format!(
            "/projects/by-remote-id/{}/search?q={}&mode={}",
            proxy_info.target_id,
            urlencoding::encode(query),
            mode_str
        );
        let response: ApiResponse<Vec<SearchResult>> = deployment
            .node_proxy_client()
            .proxy_get(&proxy_info.node_url, &path, proxy_info.node_id)
            .await?;

        return Ok(ResponseJson(response));
    }

    // Local project - execute directly
    let repo_path = &project.git_repo_path;
    let file_search_cache = deployment.file_search_cache();

    // Try cache first
    match file_search_cache
        .search(repo_path, query, mode.clone())
        .await
    {
        Ok(results) => {
            tracing::debug!(
                "Cache hit for repo {:?}, query: {}, mode: {:?}",
                repo_path,
                query,
                mode
            );
            Ok(ResponseJson(ApiResponse::success(results)))
        }
        Err(CacheError::Miss) => {
            // Cache miss - fall back to filesystem search
            tracing::debug!(
                "Cache miss for repo {:?}, query: {}, mode: {:?}",
                repo_path,
                query,
                mode
            );
            match search_files_in_repo(&project.git_repo_path.to_string_lossy(), query, mode).await
            {
                Ok(results) => Ok(ResponseJson(ApiResponse::success(results))),
                Err(e) => {
                    tracing::error!("Failed to search files: {}", e);
                    Ok(ResponseJson(ApiResponse::error(&format!(
                        "Failed to search files: {}",
                        e
                    ))))
                }
            }
        }
        Err(CacheError::BuildError(e)) => {
            tracing::error!("Cache build error for repo {:?}: {}", repo_path, e);
            // Fall back to filesystem search
            match search_files_in_repo(&project.git_repo_path.to_string_lossy(), query, mode).await
            {
                Ok(results) => Ok(ResponseJson(ApiResponse::success(results))),
                Err(e) => {
                    tracing::error!("Failed to search files: {}", e);
                    Ok(ResponseJson(ApiResponse::error(&format!(
                        "Failed to search files: {}",
                        e
                    ))))
                }
            }
        }
    }
}

async fn search_files_in_repo(
    repo_path: &str,
    query: &str,
    mode: SearchMode,
) -> Result<Vec<SearchResult>, Box<dyn std::error::Error + Send + Sync>> {
    let repo_path = StdPath::new(repo_path);

    if !repo_path.exists() {
        return Err("Repository path does not exist".into());
    }

    let mut results = Vec::new();
    let query_lower = query.to_lowercase();

    // Configure walker based on mode
    let walker = match mode {
        SearchMode::Settings => {
            // Settings mode: Include ignored files but exclude performance killers
            WalkBuilder::new(repo_path)
                .git_ignore(false) // Include ignored files like .env
                .git_global(false)
                .git_exclude(false)
                .hidden(false)
                .filter_entry(|entry| {
                    let name = entry.file_name().to_string_lossy();
                    // Always exclude .git directories and performance killers
                    name != ".git"
                        && name != "node_modules"
                        && name != "target"
                        && name != "dist"
                        && name != "build"
                })
                .build()
        }
        SearchMode::TaskForm => {
            // Task form mode: Respect gitignore (cleaner results)
            WalkBuilder::new(repo_path)
                .git_ignore(true) // Respect .gitignore
                .git_global(true) // Respect global .gitignore
                .git_exclude(true) // Respect .git/info/exclude
                .hidden(false) // Still show hidden files like .env (if not gitignored)
                .filter_entry(|entry| {
                    let name = entry.file_name().to_string_lossy();
                    name != ".git"
                })
                .build()
        }
    };

    for result in walker {
        let entry = result?;
        let path = entry.path();

        // Skip the root directory itself
        if path == repo_path {
            continue;
        }

        let relative_path = path.strip_prefix(repo_path)?;
        let relative_path_str = relative_path.to_string_lossy().to_lowercase();

        let file_name = path
            .file_name()
            .map(|name| name.to_string_lossy().to_lowercase())
            .unwrap_or_default();

        // Check for matches
        if file_name.contains(&query_lower) {
            results.push(SearchResult {
                path: relative_path.to_string_lossy().to_string(),
                is_file: path.is_file(),
                match_type: SearchMatchType::FileName,
            });
        } else if relative_path_str.contains(&query_lower) {
            // Check if it's a directory name match or full path match
            let match_type = if path
                .parent()
                .and_then(|p| p.file_name())
                .map(|name| name.to_string_lossy().to_lowercase())
                .unwrap_or_default()
                .contains(&query_lower)
            {
                SearchMatchType::DirectoryName
            } else {
                SearchMatchType::FullPath
            };

            results.push(SearchResult {
                path: relative_path.to_string_lossy().to_string(),
                is_file: path.is_file(),
                match_type,
            });
        }
    }

    // Apply git history-based ranking
    let file_ranker = FileRanker::new();
    match file_ranker.get_stats(repo_path).await {
        Ok(stats) => {
            // Re-rank results using git history
            file_ranker.rerank(&mut results, &stats);
        }
        Err(e) => {
            tracing::warn!(
                "Failed to get git stats for ranking, using basic sort: {}",
                e
            );
            // Fallback to basic priority sorting
            results.sort_by(|a, b| {
                let priority = |match_type: &SearchMatchType| match match_type {
                    SearchMatchType::FileName => 0,
                    SearchMatchType::DirectoryName => 1,
                    SearchMatchType::FullPath => 2,
                };

                priority(&a.match_type)
                    .cmp(&priority(&b.match_type))
                    .then_with(|| a.path.cmp(&b.path))
            });
        }
    }

    // Limit to top 10 results
    results.truncate(10);

    Ok(results)
}
