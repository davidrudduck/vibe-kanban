//! Remote project linking handlers.
//!
//! This module contains handlers for remote project operations:
//! - link_to_local_folder: Link a remote project to a local folder
//! - get_remote_project_by_id: Get remote project details
//! - get_project_remote_members: Get remote project members

use axum::{
    Extension, Json,
    extract::{Path, State},
    response::Json as ResponseJson,
};
use db::models::project::{CreateProject, Project, ProjectError};
use deployment::Deployment;
use utils::{
    api::projects::{RemoteProject, RemoteProjectMembersResponse},
    path::expand_tilde,
    response::ApiResponse,
};
use uuid::Uuid;

use crate::{DeploymentImpl, error::ApiError};

use super::super::types::LinkToLocalFolderRequest;
use super::core::apply_remote_project_link;

/// Create a new local project at a specified folder path and link it to a remote project.
/// This is used when a user wants to link a remote-only project to a local folder.
pub async fn link_to_local_folder(
    State(deployment): State<DeploymentImpl>,
    Json(payload): Json<LinkToLocalFolderRequest>,
) -> Result<ResponseJson<ApiResponse<Project>>, ApiError> {
    let pool = &deployment.db().pool;

    // Validate and expand the path
    let path = std::path::absolute(expand_tilde(&payload.local_folder_path))?;

    // Check if a project with this path already exists
    if let Some(existing) =
        Project::find_by_git_repo_path(pool, path.to_string_lossy().as_ref()).await?
    {
        // Project already exists at this path - just link it to the remote project
        let client = deployment.remote_client()?;
        let remote_project = client.get_project(payload.remote_project_id).await?;
        let updated_project =
            apply_remote_project_link(&deployment, existing.id, remote_project).await?;
        return Ok(ResponseJson(ApiResponse::success(updated_project)));
    }

    // Validate that the path exists and is a git repository
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

    // Get the project name - use provided name or derive from folder name
    let project_name = payload.project_name.unwrap_or_else(|| {
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("Untitled Project")
            .to_string()
    });

    // Create the local project
    let project_id = Uuid::new_v4();
    let create_data = CreateProject {
        name: project_name.clone(),
        git_repo_path: path.to_string_lossy().to_string(),
        use_existing_repo: true,
        clone_url: None,
        setup_script: None,
        dev_script: None,
        cleanup_script: None,
        copy_files: None,
    };

    let project = match Project::create(pool, &create_data, project_id).await {
        Ok(p) => p,
        Err(e) => {
            return Err(ProjectError::CreateFailed(e.to_string()).into());
        }
    };

    // Now link it to the remote project
    let client = deployment.remote_client()?;
    let remote_project = client.get_project(payload.remote_project_id).await?;
    let updated_project =
        apply_remote_project_link(&deployment, project.id, remote_project).await?;

    tracing::info!(
        project_id = %updated_project.id,
        remote_project_id = %payload.remote_project_id,
        path = %path.display(),
        "Created local project and linked to remote"
    );

    Ok(ResponseJson(ApiResponse::success(updated_project)))
}

pub async fn get_remote_project_by_id(
    State(deployment): State<DeploymentImpl>,
    Path(remote_project_id): Path<Uuid>,
) -> Result<ResponseJson<ApiResponse<RemoteProject>>, ApiError> {
    let client = deployment.remote_client()?;

    let remote_project = client.get_project(remote_project_id).await?;

    Ok(ResponseJson(ApiResponse::success(remote_project)))
}

pub async fn get_project_remote_members(
    State(deployment): State<DeploymentImpl>,
    Extension(project): Extension<Project>,
) -> Result<ResponseJson<ApiResponse<RemoteProjectMembersResponse>>, ApiError> {
    let remote_project_id = project.remote_project_id.ok_or_else(|| {
        ApiError::Conflict("Project is not linked to a remote project".to_string())
    })?;

    let client = deployment.remote_client()?;

    let remote_project = client.get_project(remote_project_id).await?;
    let members = client
        .list_members(remote_project.organization_id)
        .await?
        .members;

    Ok(ResponseJson(ApiResponse::success(
        RemoteProjectMembersResponse {
            organization_id: remote_project.organization_id,
            members,
        },
    )))
}
