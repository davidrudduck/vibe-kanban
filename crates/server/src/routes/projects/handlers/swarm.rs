//! Swarm/Hive integration handlers for projects.
//!
//! This module contains handlers for managing project connections to the Hive swarm,
//! including unlinking projects from remote swarms.

use axum::{Extension, Json, extract::State, response::Json as ResponseJson};
use db::models::{project::Project, task::Task, task_attempt::TaskAttempt};
use deployment::Deployment;
use utils::response::ApiResponse;

use crate::{DeploymentImpl, error::ApiError};

use super::super::types::{UnlinkSwarmRequest, UnlinkSwarmResponse};

/// Unlink a project from the Hive swarm.
///
/// This handler:
/// - Clears all shared_task_id values for tasks in the project
/// - Clears hive_synced_at for all task attempts in the project
/// - Sets the project's remote_project_id to NULL
/// - Optionally notifies the Hive server about the unlink
///
/// This is a cleanup operation to remove all sync references when a project
/// should no longer be connected to a remote swarm.
pub async fn unlink_from_swarm(
    State(deployment): State<DeploymentImpl>,
    Extension(project): Extension<Project>,
    Json(req): Json<UnlinkSwarmRequest>,
) -> Result<ResponseJson<ApiResponse<UnlinkSwarmResponse>>, ApiError> {
    let pool = &deployment.db().pool;
    let project_id = project.id;

    // Begin transaction for atomic operation
    let mut tx = pool.begin().await?;

    // Clear all shared_task_id values for tasks in this project
    let tasks_unlinked =
        Task::clear_all_shared_task_ids_for_project_tx(&mut *tx, project_id).await?;

    // Clear hive_synced_at for all task attempts
    let attempts_reset = TaskAttempt::clear_hive_sync_for_project_tx(&mut *tx, project_id).await?;

    // Clear the project's remote_project_id
    Project::set_remote_project_id_tx(&mut *tx, project_id, None).await?;

    // Commit transaction - all succeed or all rollback
    tx.commit().await?;

    // TODO: Implement Hive notification when notify_hive is true
    // For now, we'll just log and set hive_notified to false
    let hive_notified = if req.notify_hive {
        tracing::warn!(project_id = %project_id, "Hive notification requested but not yet implemented");
        false
    } else {
        false
    };

    tracing::info!(
        project_id = %project_id,
        tasks_unlinked,
        attempts_reset,
        hive_notified,
        "Project unlinked from swarm"
    );

    Ok(ResponseJson(ApiResponse::success(UnlinkSwarmResponse {
        tasks_unlinked,
        attempts_reset,
        hive_notified,
    })))
}
