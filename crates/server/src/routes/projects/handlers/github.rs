//! GitHub integration handlers for projects.
//!
//! This module contains handlers for GitHub integration:
//! - set_github_enabled: Enable/disable GitHub integration
//! - get_github_counts: Get GitHub issue/PR counts
//! - sync_github_counts: Trigger manual sync of GitHub counts

use axum::{Extension, Json, extract::State, response::Json as ResponseJson};
use db::models::project::{Project, ProjectError};
use deployment::Deployment;
use utils::response::ApiResponse;

use crate::{DeploymentImpl, error::ApiError};

use super::super::types::{GitHubCountsResponse, SetGitHubEnabledRequest};

/// Enable or disable GitHub integration for a project
pub async fn set_github_enabled(
    Extension(project): Extension<Project>,
    State(deployment): State<DeploymentImpl>,
    Json(payload): Json<SetGitHubEnabledRequest>,
) -> Result<ResponseJson<ApiResponse<Project>>, ApiError> {
    // Validate: if enabling, owner and repo must be provided
    if payload.enabled && (payload.owner.is_none() || payload.repo.is_none()) {
        return Ok(ResponseJson(ApiResponse::error(
            "GitHub owner and repo are required when enabling GitHub integration",
        )));
    }

    // Update the database
    Project::set_github_enabled(
        &deployment.db().pool,
        project.id,
        payload.enabled,
        payload.owner.clone(),
        payload.repo.clone(),
    )
    .await?;

    // If enabling, trigger an immediate sync
    if payload.enabled {
        let updated_project = Project::find_by_id(&deployment.db().pool, project.id)
            .await?
            .ok_or(ProjectError::ProjectNotFound)?;

        // Sync in the background so we don't block the response
        let db = deployment.db().clone();
        let project_clone = updated_project.clone();
        tokio::spawn(async move {
            if let Err(e) =
                services::services::github_sync::sync_single_project(&db, &project_clone).await
            {
                tracing::warn!(
                    project_id = %project_clone.id,
                    "Failed to sync GitHub counts on enable: {}",
                    e
                );
            }
        });

        return Ok(ResponseJson(ApiResponse::success(updated_project)));
    }

    // Return the updated project
    let updated_project = Project::find_by_id(&deployment.db().pool, project.id)
        .await?
        .ok_or(ProjectError::ProjectNotFound)?;

    Ok(ResponseJson(ApiResponse::success(updated_project)))
}

/// Get current GitHub counts for a project
pub async fn get_github_counts(
    Extension(project): Extension<Project>,
) -> Result<ResponseJson<ApiResponse<GitHubCountsResponse>>, ApiError> {
    Ok(ResponseJson(ApiResponse::success(GitHubCountsResponse {
        open_issues: project.github_open_issues,
        open_prs: project.github_open_prs,
        last_synced_at: project.github_last_synced_at,
    })))
}

/// Trigger an immediate sync of GitHub counts for a project
pub async fn sync_github_counts(
    Extension(project): Extension<Project>,
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<GitHubCountsResponse>>, ApiError> {
    if !project.github_enabled {
        return Ok(ResponseJson(ApiResponse::error(
            "GitHub integration is not enabled for this project",
        )));
    }

    // Trigger sync
    services::services::github_sync::sync_single_project(deployment.db(), &project)
        .await
        .map_err(|e| ApiError::BadRequest(format!("Failed to sync GitHub counts: {}", e)))?;

    // Fetch updated project
    let updated_project = Project::find_by_id(&deployment.db().pool, project.id)
        .await?
        .ok_or(ProjectError::ProjectNotFound)?;

    Ok(ResponseJson(ApiResponse::success(GitHubCountsResponse {
        open_issues: updated_project.github_open_issues,
        open_prs: updated_project.github_open_prs,
        last_synced_at: updated_project.github_last_synced_at,
    })))
}
