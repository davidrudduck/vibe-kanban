use axum::{
    Extension, Json,
    extract::{Query, State},
    http::StatusCode,
    response::Json as ResponseJson,
};
use db::models::{
    coding_agent_turn::CodingAgentTurn,
    execution_process::{ExecutionProcess, ExecutionProcessStatus},
    workspace::{Workspace, WorkspaceError},
    workspace_repo::WorkspaceRepo,
};
use deployment::Deployment;
use serde::Deserialize;
use services::services::{container::ContainerService, diff_stream, remote_sync};
use sqlx::Error as SqlxError;
use utils::response::ApiResponse;
use workspace_manager::WorkspaceManager;

use crate::{DeploymentImpl, error::ApiError};

fn workspace_repo_dirs(workspace: &Workspace) -> Vec<std::path::PathBuf> {
    let Some(ref container_ref) = workspace.container_ref else {
        return Vec::new();
    };
    if workspace.worktree_deleted {
        return Vec::new();
    }

    let workspace_path = std::path::PathBuf::from(container_ref);
    if !workspace_path.exists() {
        return Vec::new();
    }

    let subdirs: Vec<std::path::PathBuf> = std::fs::read_dir(&workspace_path)
        .ok()
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .map(|e| e.path())
        .collect();

    if subdirs.is_empty() {
        vec![workspace_path]
    } else {
        subdirs
    }
}

fn ensure_workspace_worktrees_clean(
    deployment: &DeploymentImpl,
    workspace: &Workspace,
    force: bool,
    force_hint: &str,
) -> Result<(), ApiError> {
    if force {
        return Ok(());
    }

    for dir in workspace_repo_dirs(workspace) {
        match deployment.git().is_worktree_clean(&dir) {
            Ok(true) => {}
            Ok(false) => {
                return Err(ApiError::Conflict(format!(
                    "Workspace has uncommitted changes. Pass {force_hint}=true to continue anyway."
                )));
            }
            Err(e) => {
                tracing::warn!(
                    workspace_id = %workspace.id,
                    dir = %dir.display(),
                    error = ?e,
                    "Could not determine git dirty status; proceeding"
                );
            }
        }
    }

    Ok(())
}

async fn cleanup_workspace_worktrees(
    pool: &sqlx::SqlitePool,
    workspace: &Workspace,
) -> Result<(), ApiError> {
    let Some(container_ref) = &workspace.container_ref else {
        return Ok(());
    };
    if workspace.worktree_deleted {
        return Ok(());
    }

    let workspace_dir = std::path::PathBuf::from(container_ref);
    let repositories = WorkspaceRepo::find_repos_for_workspace(pool, workspace.id).await?;

    if repositories.is_empty() {
        if workspace_dir.exists() {
            tokio::fs::remove_dir_all(&workspace_dir).await?;
        }
    } else {
        WorkspaceManager::cleanup_workspace(&workspace_dir, &repositories).await?;
    }

    Workspace::mark_worktree_deleted(pool, workspace.id).await?;
    Ok(())
}

#[derive(Debug, Deserialize)]
pub struct DeleteWorkspaceQuery {
    #[serde(default)]
    pub delete_remote: bool,
    #[serde(default)]
    pub delete_branches: bool,
    #[serde(default)]
    pub force_delete: bool,
}

#[cfg(test)]
mod tests {
    use super::DeleteWorkspaceQuery;

    #[test]
    fn delete_workspace_query_deserializes_force_delete() {
        let query: DeleteWorkspaceQuery = serde_json::from_value(serde_json::json!({
            "delete_branches": true,
            "force_delete": true
        }))
        .unwrap();

        assert!(query.delete_branches);
        assert!(query.force_delete);
    }
}

pub async fn get_workspaces(
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<Vec<Workspace>>>, ApiError> {
    let pool = &deployment.db().pool;
    let workspaces = Workspace::fetch_all(pool).await?;
    Ok(ResponseJson(ApiResponse::success(workspaces)))
}

pub async fn get_workspace(
    Extension(workspace): Extension<Workspace>,
) -> Result<ResponseJson<ApiResponse<Workspace>>, ApiError> {
    Ok(ResponseJson(ApiResponse::success(workspace)))
}

pub async fn update_workspace(
    Extension(workspace): Extension<Workspace>,
    State(deployment): State<DeploymentImpl>,
    Json(request): Json<db::models::requests::UpdateWorkspace>,
) -> Result<ResponseJson<ApiResponse<Workspace>>, ApiError> {
    let pool = &deployment.db().pool;
    let is_archiving = request.archived == Some(true) && !workspace.archived;

    if is_archiving {
        ensure_workspace_worktrees_clean(
            &deployment,
            &workspace,
            request.force_archive,
            "force_archive",
        )?;
    }

    Workspace::update(
        pool,
        workspace.id,
        request.archived,
        request.pinned,
        request.name.as_deref(),
    )
    .await?;
    let updated = Workspace::find_by_id(pool, workspace.id)
        .await?
        .ok_or(WorkspaceError::WorkspaceNotFound)?;

    if (request.archived.is_some() || request.name.is_some())
        && let Ok(client) = deployment.remote_client()
    {
        let ws = updated.clone();
        let name = request.name.clone();
        let archived = request.archived;
        let stats =
            diff_stream::compute_diff_stats(&deployment.db().pool, deployment.git(), &ws).await;
        tokio::spawn(async move {
            remote_sync::sync_workspace_to_remote(
                &client,
                ws.id,
                name.map(Some),
                archived,
                stats.as_ref(),
            )
            .await;
        });
    }

    if is_archiving {
        if let Err(e) = deployment.container().archive_workspace(workspace.id).await {
            tracing::error!("Failed to archive workspace {}: {}", workspace.id, e);
        }
        cleanup_workspace_worktrees(pool, &updated).await?;
    }

    Ok(ResponseJson(ApiResponse::success(updated)))
}

pub async fn get_first_user_message(
    Extension(workspace): Extension<Workspace>,
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<Option<String>>>, ApiError> {
    let pool = &deployment.db().pool;
    let message = Workspace::get_first_user_message(pool, workspace.id).await?;
    Ok(ResponseJson(ApiResponse::success(message)))
}

pub async fn delete_workspace(
    Extension(workspace): Extension<Workspace>,
    State(deployment): State<DeploymentImpl>,
    Query(query): Query<DeleteWorkspaceQuery>,
) -> Result<(StatusCode, ResponseJson<ApiResponse<()>>), ApiError> {
    let pool = &deployment.db().pool;
    let workspace_manager = deployment.workspace_manager();
    let workspace_id = workspace.id;

    if ExecutionProcess::has_running_non_dev_server_processes_for_workspace(pool, workspace_id)
        .await?
    {
        return Err(ApiError::Conflict(
            "Cannot delete workspace while processes are running. Stop all processes first."
                .to_string(),
        ));
    }

    ensure_workspace_worktrees_clean(&deployment, &workspace, query.force_delete, "force_delete")?;

    let dev_servers =
        ExecutionProcess::find_running_dev_servers_by_workspace(pool, workspace_id).await?;

    for dev_server in dev_servers {
        tracing::info!(
            "Stopping dev server {} before deleting workspace {}",
            dev_server.id,
            workspace_id
        );

        if let Err(e) = deployment
            .container()
            .stop_execution(&dev_server, ExecutionProcessStatus::Killed)
            .await
        {
            tracing::error!(
                "Failed to stop dev server {} for workspace {}: {}",
                dev_server.id,
                workspace_id,
                e
            );
        }
    }

    let managed_workspace = workspace_manager.load_managed_workspace(workspace).await?;
    let deletion_context = managed_workspace.prepare_deletion_context().await?;
    let rows_affected = managed_workspace.delete_record().await?;

    if rows_affected == 0 {
        return Err(ApiError::Database(SqlxError::RowNotFound));
    }

    deployment
        .track_if_analytics_allowed(
            "workspace_deleted",
            serde_json::json!({
                "workspace_id": workspace_id.to_string(),
            }),
        )
        .await;

    if query.delete_remote {
        if let Ok(client) = deployment.remote_client() {
            match client.delete_workspace(workspace_id).await {
                Ok(()) => {
                    tracing::info!("Deleted remote workspace for {}", workspace_id);
                }
                Err(e) => {
                    tracing::warn!(
                        "Failed to delete remote workspace for {}: {}",
                        workspace_id,
                        e
                    );
                }
            }
        } else {
            tracing::debug!(
                "Remote client not available, skipping remote deletion for {}",
                workspace_id
            );
        }
    }

    WorkspaceManager::spawn_workspace_deletion_cleanup(deletion_context, query.delete_branches);

    Ok((StatusCode::ACCEPTED, ResponseJson(ApiResponse::success(()))))
}

#[axum::debug_handler]
pub async fn mark_seen(
    Extension(workspace): Extension<Workspace>,
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<()>>, ApiError> {
    let pool = &deployment.db().pool;
    CodingAgentTurn::mark_seen_by_workspace_id(pool, workspace.id).await?;
    Ok(ResponseJson(ApiResponse::success(())))
}
