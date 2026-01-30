//! Swarm/Hive integration handlers for projects.
//!
//! This module contains handlers for managing project connections to the Hive swarm,
//! including unlinking projects from remote swarms.

use axum::{Extension, Json, extract::State, response::Json as ResponseJson};
use db::models::{project::Project, task::Task, task_attempt::TaskAttempt};
use deployment::Deployment;
use utils::response::ApiResponse;

use crate::{DeploymentImpl, error::ApiError};

use super::super::types::{ForceResyncResponse, UnlinkSwarmRequest, UnlinkSwarmResponse};

/// Remove a project's Hive synchronization metadata and detach it from its remote swarm.
///
/// Performs three related updates atomically: clears tasks' `shared_task_id` values,
/// clears `hive_synced_at` for all task attempts, and sets the project's
/// `remote_project_id` to `NULL`. If `req.notify_hive` is true, a Hive notification
/// would be attempted in the future; currently the handler logs a warning and
/// returns `hive_notified = false`.
///
/// # Parameters
///
/// * `req` — Controls whether a Hive notification should be attempted (`notify_hive`).
///
/// # Returns
///
/// An `ApiResponse` wrapping `UnlinkSwarmResponse` with:
/// * `tasks_unlinked` — number of tasks that had `shared_task_id` cleared.
/// * `attempts_reset` — number of task attempts that had `hive_synced_at` cleared.
/// * `hive_notified` — `false` (notification is not implemented).
///
/// # Examples
///
/// ```
/// // Construct the expected response payload (example, not an invocation).
/// let resp = UnlinkSwarmResponse {
///     tasks_unlinked: 42,
///     attempts_reset: 10,
///     hive_notified: false,
/// };
/// assert_eq!(resp.hive_notified, false);
/// ```
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

/// Marks all tasks in the given project to be re-synchronized with the Hive.
///
/// This clears each synced task's `remote_last_synced_at` (for tasks with a `shared_task_id`) so the next sync cycle will resend their `TaskSyncMessage` including refreshed fields such as labels, `owner_node_id`, and assignee. Use this when new sync fields are introduced or to recover from missing/incorrect synced data.
///
/// # Returns
///
/// An `ApiResponse<ForceResyncResponse>` whose `tasks_resynced` field is the number of tasks that were marked for resync.
///
/// # Examples
///
/// ```
/// use crates::server::routes::projects::handlers::swarm::ForceResyncResponse;
///
/// let resp = ForceResyncResponse { tasks_resynced: 42 };
/// assert_eq!(resp.tasks_resynced, 42);
/// ```
pub async fn force_resync_tasks(
    State(deployment): State<DeploymentImpl>,
    Extension(project): Extension<Project>,
) -> Result<ResponseJson<ApiResponse<ForceResyncResponse>>, ApiError> {
    let pool = &deployment.db().pool;
    let project_id = project.id;

    // Mark all synced tasks for resync
    let tasks_marked = Task::mark_for_resync_by_project(pool, project_id).await?;

    tracing::info!(
        project_id = %project_id,
        tasks_marked,
        "Marked tasks for force resync"
    );

    Ok(ResponseJson(ApiResponse::success(ForceResyncResponse {
        tasks_resynced: tasks_marked as usize,
    })))
}