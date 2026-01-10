//! Core CRUD operations for projects.
//!
//! This module contains handlers for basic project operations:
//! - list, get, create, update, delete
//! - orphaned project management
//! - project config scanning
//! - branch listing
//! - editor opening

use axum::{
    Extension, Json,
    extract::State,
    http::StatusCode,
    response::Json as ResponseJson,
};
use db::models::project::{
    CreateProject, Project, ProjectError, ScanConfigRequest, ScanConfigResponse, UpdateProject,
};
use deployment::Deployment;
use services::services::{
    git::GitBranch,
    project_detector::ProjectDetector,
    remote_client::CreateRemoteProjectPayload,
    share::share_existing_tasks_to_hive,
};
use utils::{api::projects::RemoteProject, path::expand_tilde, response::ApiResponse};
use uuid::Uuid;

use crate::{DeploymentImpl, error::ApiError, middleware::RemoteProjectContext};

use super::super::types::{OpenEditorRequest, OpenEditorResponse, OrphanedProject, OrphanedProjectsResponse};

// Re-export for use in this module
use crate::proxy::check_remote_proxy;

/// Helper function to truncate node name at first period
pub fn truncate_node_name(name: &str) -> String {
    name.split('.').next().unwrap_or(name).to_string()
}

pub async fn get_projects(
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<Vec<Project>>>, ApiError> {
    let projects = Project::find_all(&deployment.db().pool).await?;
    Ok(ResponseJson(ApiResponse::success(projects)))
}

pub async fn get_project(
    Extension(project): Extension<Project>,
) -> Result<ResponseJson<ApiResponse<Project>>, ApiError> {
    Ok(ResponseJson(ApiResponse::success(project)))
}

pub async fn get_project_branches(
    Extension(project): Extension<Project>,
    remote_ctx: Option<Extension<RemoteProjectContext>>,
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<Vec<GitBranch>>>, ApiError> {
    // LOCAL-FIRST: If we have a valid local git repo, use it directly
    // This handles the case where a project exists on multiple nodes and this node has a local copy
    if !project.git_repo_path.as_os_str().is_empty()
        && project.git_repo_path.exists()
        && project.git_repo_path.join(".git").exists()
    {
        tracing::debug!(
            project_id = %project.id,
            git_repo_path = %project.git_repo_path.display(),
            "Using local git repository for branches"
        );
        let branches = deployment.git().get_all_branches(&project.git_repo_path)?;
        return Ok(ResponseJson(ApiResponse::success(branches)));
    }

    // Fall back to proxy only if no local git repository exists
    if let Some(proxy_info) = check_remote_proxy(remote_ctx.as_ref().map(|e| &e.0))? {
        tracing::debug!(
            node_id = %proxy_info.node_id,
            remote_project_id = %proxy_info.target_id,
            "Proxying get_project_branches to remote node"
        );

        let path = format!("/projects/by-remote-id/{}/branches", proxy_info.target_id);
        let response: ApiResponse<Vec<GitBranch>> = deployment
            .node_proxy_client()
            .proxy_get(&proxy_info.node_url, &path, proxy_info.node_id)
            .await?;

        return Ok(ResponseJson(response));
    }

    // No local git and no remote to proxy to
    Err(ApiError::BadRequest(
        "No git repository available for this project".to_string(),
    ))
}

pub async fn create_project(
    State(deployment): State<DeploymentImpl>,
    Json(payload): Json<CreateProject>,
) -> Result<ResponseJson<ApiResponse<Project>>, ApiError> {
    let id = Uuid::new_v4();
    let CreateProject {
        name,
        git_repo_path,
        setup_script,
        dev_script,
        cleanup_script,
        copy_files,
        use_existing_repo,
        clone_url,
    } = payload;
    tracing::debug!("Creating project '{}'", name);

    // Validate and setup git repository
    let path = std::path::absolute(expand_tilde(&git_repo_path))?;
    // Check if git repo path is already used by another project
    match Project::find_by_git_repo_path(&deployment.db().pool, path.to_string_lossy().as_ref())
        .await
    {
        Ok(Some(_)) => {
            return Ok(ResponseJson(ApiResponse::error(
                "A project with this git repository path already exists",
            )));
        }
        Ok(None) => {
            // Path is available, continue
        }
        Err(e) => {
            return Err(ProjectError::GitRepoCheckFailed(e.to_string()).into());
        }
    }

    // Handle repository setup based on mode
    if let Some(url) = clone_url.as_ref() {
        // Clone from URL mode
        if use_existing_repo {
            return Ok(ResponseJson(ApiResponse::error(
                "Cannot use both clone_url and use_existing_repo",
            )));
        }

        tracing::info!(clone_url = %url, dest = %path.display(), "Cloning repository");

        if let Err(e) = deployment.git().clone_repo(url, &path) {
            tracing::error!("Failed to clone repository: {}", e);
            return Ok(ResponseJson(ApiResponse::error(&format!(
                "Failed to clone repository: {}",
                e
            ))));
        }
    } else if use_existing_repo {
        // For existing repos, validate that the path exists and is a git repository
        if !path.exists() {
            return Ok(ResponseJson(ApiResponse::error(
                "The specified path does not exist",
            )));
        }

        if !path.is_dir() {
            return Ok(ResponseJson(ApiResponse::error(
                "The specified path is not a directory",
            )));
        }

        if !path.join(".git").exists() {
            return Ok(ResponseJson(ApiResponse::error(
                "The specified directory is not a git repository",
            )));
        }

        // Ensure existing repo has a main branch if it's empty
        if let Err(e) = deployment.git().ensure_main_branch_exists(&path) {
            tracing::error!("Failed to ensure main branch exists: {}", e);
            return Ok(ResponseJson(ApiResponse::error(&format!(
                "Failed to ensure main branch exists: {}",
                e
            ))));
        }
    } else {
        // For new repos, create directory and initialize git

        // Create directory if it doesn't exist
        if !path.exists()
            && let Err(e) = std::fs::create_dir_all(&path)
        {
            tracing::error!("Failed to create directory: {}", e);
            return Ok(ResponseJson(ApiResponse::error(&format!(
                "Failed to create directory: {}",
                e
            ))));
        }

        // Check if it's already a git repo, if not initialize it
        if !path.join(".git").exists()
            && let Err(e) = deployment.git().initialize_repo_with_main_branch(&path)
        {
            tracing::error!("Failed to initialize git repository: {}", e);
            return Ok(ResponseJson(ApiResponse::error(&format!(
                "Failed to initialize git repository: {}",
                e
            ))));
        }
    }

    let project = match Project::create(
        &deployment.db().pool,
        &CreateProject {
            name: name.clone(),
            git_repo_path: path.to_string_lossy().to_string(),
            use_existing_repo,
            clone_url,
            setup_script,
            dev_script,
            cleanup_script,
            copy_files,
        },
        id,
    )
    .await
    {
        Ok(project) => project,
        Err(e) => return Err(ProjectError::CreateFailed(e.to_string()).into()),
    };

    // Auto-link to hive if connected
    let final_project = auto_link_project_to_hive(&deployment, project, &name, &path).await;

    Ok(ResponseJson(ApiResponse::success(final_project)))
}

/// Automatically link a newly created project to the hive if connected.
/// This creates a remote project and links it so all nodes in the swarm can see it.
async fn auto_link_project_to_hive(
    deployment: &DeploymentImpl,
    project: Project,
    name: &str,
    path: &std::path::Path,
) -> Project {
    // Check if we're connected to a hive
    let ctx = match deployment.node_runner_context() {
        Some(ctx) => ctx,
        None => {
            tracing::debug!(
                project_id = %project.id,
                "No node runner context, skipping auto-link"
            );
            return project;
        }
    };

    if !ctx.is_connected().await {
        tracing::debug!(
            project_id = %project.id,
            "Not connected to hive, skipping auto-link"
        );
        return project;
    }

    // Get the organization ID from the hive connection
    let organization_id = match ctx.organization_id().await {
        Some(org_id) => org_id,
        None => {
            tracing::warn!(
                project_id = %project.id,
                "Connected to hive but no organization_id available, skipping auto-link"
            );
            return project;
        }
    };

    // Get the remote client
    let client = match deployment.remote_client() {
        Ok(client) => client,
        Err(e) => {
            tracing::warn!(
                project_id = %project.id,
                error = %e,
                "Failed to get remote client for auto-link"
            );
            return project;
        }
    };

    // Create a remote project in the hive
    let remote_project = match client
        .create_project(&CreateRemoteProjectPayload {
            organization_id,
            name: name.to_string(),
            metadata: None,
        })
        .await
    {
        Ok(remote_project) => remote_project,
        Err(e) => {
            tracing::warn!(
                project_id = %project.id,
                error = %e,
                "Failed to create remote project in hive for auto-link"
            );
            return project;
        }
    };

    tracing::info!(
        project_id = %project.id,
        remote_project_id = %remote_project.id,
        "Created remote project in hive, now linking"
    );

    // Link the local project to the remote project
    match apply_remote_project_link(deployment, project.id, remote_project).await {
        Ok(updated_project) => {
            tracing::info!(
                project_id = %updated_project.id,
                remote_project_id = ?updated_project.remote_project_id,
                path = %path.display(),
                "Auto-linked new project to hive"
            );
            updated_project
        }
        Err(e) => {
            tracing::warn!(
                project_id = %project.id,
                error = %e,
                "Failed to apply remote project link during auto-link"
            );
            project
        }
    }
}

/// Apply a remote project link to a local project.
/// This is a shared helper used by both create_project (auto-link) and linking handlers.
pub async fn apply_remote_project_link(
    deployment: &DeploymentImpl,
    project_id: Uuid,
    remote_project: RemoteProject,
) -> Result<Project, ApiError> {
    let pool = &deployment.db().pool;

    // First get the project to get git_repo_path for the hive notification
    let project = Project::find_by_id(pool, project_id)
        .await?
        .ok_or(ProjectError::ProjectNotFound)?;

    Project::set_remote_project_id(pool, project_id, Some(remote_project.id)).await?;

    let updated_project = Project::find_by_id(pool, project_id)
        .await?
        .ok_or(ProjectError::ProjectNotFound)?;

    let current_profile = deployment.auth_context().cached_profile().await;
    let current_user_id = current_profile.as_ref().map(|p| p.user_id);

    // Note: Pulling shared tasks from Hive is now handled by ElectricSQL.
    // We only push existing local tasks to the Hive here.
    if let Ok(publisher) = deployment.share_publisher() {
        match share_existing_tasks_to_hive(pool, &publisher, project_id, current_user_id).await {
            Ok(count) => {
                tracing::info!(
                    project_id = %project_id,
                    shared_count = count,
                    "Shared existing tasks to Hive during project linking"
                );
            }
            Err(e) => {
                tracing::warn!(
                    project_id = %project_id,
                    error = ?e,
                    "Failed to share existing tasks to Hive during project linking"
                );
            }
        }
    }

    // Notify the hive about the link if we're connected as a node
    if let Some(ctx) = deployment.node_runner_context() {
        use services::services::hive_client::LinkProjectMessage;

        if ctx.is_connected().await {
            // Get the current branch from git (use as default branch)
            let default_branch = deployment
                .git()
                .get_current_branch(&project.git_repo_path)
                .unwrap_or_else(|_| "main".to_string());

            let link_msg = LinkProjectMessage {
                project_id: remote_project.id,
                local_project_id: project_id,
                git_repo_path: project.git_repo_path.to_string_lossy().to_string(),
                default_branch,
            };

            if let Err(e) = ctx.send_link_project(link_msg).await {
                tracing::warn!(
                    project_id = %project_id,
                    remote_project_id = %remote_project.id,
                    error = %e,
                    "failed to send link_project to hive"
                );
            } else {
                tracing::info!(
                    project_id = %project_id,
                    remote_project_id = %remote_project.id,
                    "sent link_project to hive"
                );
            }
        }
    }

    Ok(updated_project)
}

pub async fn update_project(
    Extension(existing_project): Extension<Project>,
    State(deployment): State<DeploymentImpl>,
    Json(payload): Json<UpdateProject>,
) -> Result<ResponseJson<ApiResponse<Project>>, StatusCode> {
    // Destructure payload to handle field updates.
    // This allows us to treat `None` from the payload as an explicit `null` to clear a field,
    // as the frontend currently sends all fields on update.
    let UpdateProject {
        name,
        git_repo_path,
        setup_script,
        dev_script,
        cleanup_script,
        copy_files,
        parallel_setup_script,
    } = payload;
    // If git_repo_path is being changed, check if the new path is already used by another project
    let git_repo_path = if let Some(new_git_repo_path) = git_repo_path.map(|s| expand_tilde(&s))
        && new_git_repo_path != existing_project.git_repo_path
    {
        match Project::find_by_git_repo_path_excluding_id(
            &deployment.db().pool,
            new_git_repo_path.to_string_lossy().as_ref(),
            existing_project.id,
        )
        .await
        {
            Ok(Some(_)) => {
                return Ok(ResponseJson(ApiResponse::error(
                    "A project with this git repository path already exists",
                )));
            }
            Ok(None) => new_git_repo_path,
            Err(e) => {
                tracing::error!("Failed to check for existing git repo path: {}", e);
                return Err(StatusCode::INTERNAL_SERVER_ERROR);
            }
        }
    } else {
        existing_project.git_repo_path
    };

    match Project::update(
        &deployment.db().pool,
        existing_project.id,
        name.unwrap_or(existing_project.name),
        git_repo_path.to_string_lossy().to_string(),
        setup_script,
        dev_script,
        cleanup_script,
        copy_files,
        parallel_setup_script.unwrap_or(existing_project.parallel_setup_script),
    )
    .await
    {
        Ok(project) => Ok(ResponseJson(ApiResponse::success(project))),
        Err(e) => {
            tracing::error!("Failed to update project: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn delete_project(
    Extension(project): Extension<Project>,
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<()>>, StatusCode> {
    match Project::delete(&deployment.db().pool, project.id).await {
        Ok(rows_affected) => {
            if rows_affected == 0 {
                Err(StatusCode::NOT_FOUND)
            } else {
                Ok(ResponseJson(ApiResponse::success(())))
            }
        }
        Err(e) => {
            tracing::error!("Failed to delete project: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// GET /api/projects/orphaned - List projects with non-existent git_repo_path
pub async fn list_orphaned_projects(
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<OrphanedProjectsResponse>>, ApiError> {
    let all_projects = Project::find_all(&deployment.db().pool).await?;

    let orphaned: Vec<OrphanedProject> = all_projects
        .into_iter()
        .filter(|p| {
            let path = std::path::Path::new(&p.git_repo_path);
            // Only check local projects (remote projects have paths from other nodes)
            !p.is_remote && !path.exists()
        })
        .map(|p| OrphanedProject {
            id: p.id,
            name: p.name,
            git_repo_path: p.git_repo_path.display().to_string(),
            is_remote: p.is_remote,
        })
        .collect();

    let count = orphaned.len();
    Ok(ResponseJson(ApiResponse::success(OrphanedProjectsResponse {
        projects: orphaned,
        count,
    })))
}

/// DELETE /api/projects/orphaned - Remove projects with non-existent git_repo_path
pub async fn delete_orphaned_projects(
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<OrphanedProjectsResponse>>, ApiError> {
    let all_projects = Project::find_all(&deployment.db().pool).await?;

    let mut deleted: Vec<OrphanedProject> = Vec::new();

    for p in all_projects {
        let path = std::path::Path::new(&p.git_repo_path);
        // Only delete local projects (remote projects have paths from other nodes)
        if !p.is_remote && !path.exists() {
            if let Err(e) = Project::delete(&deployment.db().pool, p.id).await {
                tracing::error!(project_id = %p.id, "Failed to delete orphaned project: {}", e);
            } else {
                tracing::info!(
                    project_id = %p.id,
                    path = %p.git_repo_path.display(),
                    "Deleted orphaned project with non-existent path"
                );
                deleted.push(OrphanedProject {
                    id: p.id,
                    name: p.name,
                    git_repo_path: p.git_repo_path.display().to_string(),
                    is_remote: p.is_remote,
                });
            }
        }
    }

    let count = deleted.len();
    Ok(ResponseJson(ApiResponse::success(OrphanedProjectsResponse {
        projects: deleted,
        count,
    })))
}

pub async fn open_project_in_editor(
    Extension(project): Extension<Project>,
    State(deployment): State<DeploymentImpl>,
    Json(payload): Json<Option<OpenEditorRequest>>,
) -> Result<ResponseJson<ApiResponse<OpenEditorResponse>>, ApiError> {
    let path = project.git_repo_path;

    let editor_config = {
        let config = deployment.config().read().await;
        let editor_type_str = payload.as_ref().and_then(|req| req.editor_type.as_deref());
        config.editor.with_override(editor_type_str)
    };

    match editor_config.open_file(&path).await {
        Ok(url) => {
            tracing::info!(
                "Opened editor for project {} at path: {}{}",
                project.id,
                path.to_string_lossy(),
                if url.is_some() { " (remote mode)" } else { "" }
            );

            Ok(ResponseJson(ApiResponse::success(OpenEditorResponse {
                url,
            })))
        }
        Err(e) => {
            tracing::error!("Failed to open editor for project {}: {:?}", project.id, e);
            Err(ApiError::EditorOpen(e))
        }
    }
}

pub async fn scan_project_config(
    Json(payload): Json<ScanConfigRequest>,
) -> Result<ResponseJson<ApiResponse<ScanConfigResponse>>, ApiError> {
    let repo_path = std::path::absolute(expand_tilde(&payload.repo_path))?;

    if !repo_path.exists() {
        return Ok(ResponseJson(ApiResponse::error(
            "Repository path does not exist",
        )));
    }

    match ProjectDetector::scan_repo(&repo_path) {
        Ok(suggestions) => Ok(ResponseJson(ApiResponse::success(ScanConfigResponse {
            suggestions,
        }))),
        Err(e) => {
            tracing::error!("Failed to scan project config: {}", e);
            Err(ApiError::BadRequest(format!(
                "Failed to scan project: {}",
                e
            )))
        }
    }
}
