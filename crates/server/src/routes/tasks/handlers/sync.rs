//! Sync handlers: push local archive status to Hive.
//!
//! These are endpoints for one-time sync operations that fix historical data issues.

use axum::{extract::State, response::Json as ResponseJson};
use db::models::task::Task;
use deployment::Deployment;
use remote::routes::tasks::UpdateSharedTaskRequest;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use utils::response::ApiResponse;

use crate::{DeploymentImpl, error::ApiError};

// ============================================================================
// Types
// ============================================================================

/// Response from the sync-archived-to-hive endpoint.
#[derive(Debug, Serialize, Deserialize, TS)]
pub struct SyncArchivedToHiveResponse {
    /// Number of tasks successfully synced
    pub synced_count: u64,
    /// Number of tasks that failed to sync
    pub failed_count: u64,
    /// Error messages for failed tasks (task_id -> error)
    pub errors: Vec<SyncError>,
}

/// Error detail for a single task sync failure.
#[derive(Debug, Serialize, Deserialize, TS)]
pub struct SyncError {
    pub task_id: String,
    pub shared_task_id: String,
    pub error: String,
}

// ============================================================================
// Sync Archived Tasks to Hive
// ============================================================================

/// Push local archived_at values to Hive for tasks that were archived locally
/// but never had their archive status synced to Hive.
///
/// This is a one-time backfill endpoint to fix historical data where tasks were
/// archived before getting a shared_task_id or before archive status was synced.
///
/// The endpoint finds tasks where:
/// - `archived_at` is NOT NULL (task is archived locally)
/// - `shared_task_id` is NOT NULL (task is synced to Hive)
/// - The task belongs to a swarm project
///
/// For each such task, it calls the Hive API to update the archived_at value.
pub async fn sync_archived_to_hive(
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<SyncArchivedToHiveResponse>>, ApiError> {
    let pool = &deployment.db().pool;
    let remote_client = deployment.remote_client()?;

    // Find all archived tasks with shared_task_id that belong to swarm projects
    let tasks = Task::find_archived_needing_hive_sync(pool, 1000).await?;

    tracing::info!(
        task_count = tasks.len(),
        "Starting sync of archived tasks to Hive"
    );

    let mut synced_count = 0u64;
    let mut failed_count = 0u64;
    let mut errors = Vec::new();

    for task in tasks {
        let shared_task_id = match task.shared_task_id {
            Some(id) => id,
            None => {
                // This shouldn't happen given the query, but skip if it does
                tracing::warn!(task_id = %task.id, "Task missing shared_task_id, skipping");
                continue;
            }
        };

        let archived_at = match task.archived_at {
            Some(ts) => ts,
            None => {
                // This shouldn't happen given the query, but skip if it does
                tracing::warn!(task_id = %task.id, "Task missing archived_at, skipping");
                continue;
            }
        };

        // Push the archived_at value to Hive
        let request = UpdateSharedTaskRequest {
            title: None,
            description: None,
            status: None,
            archived_at: Some(Some(archived_at)),
            version: None, // Don't use version - this is a backfill operation
        };

        match remote_client
            .update_shared_task(shared_task_id, &request)
            .await
        {
            Ok(_response) => {
                tracing::debug!(
                    task_id = %task.id,
                    shared_task_id = %shared_task_id,
                    archived_at = %archived_at,
                    "Successfully synced archived_at to Hive"
                );
                synced_count += 1;
            }
            Err(e) => {
                tracing::warn!(
                    task_id = %task.id,
                    shared_task_id = %shared_task_id,
                    error = %e,
                    "Failed to sync archived_at to Hive"
                );
                errors.push(SyncError {
                    task_id: task.id.to_string(),
                    shared_task_id: shared_task_id.to_string(),
                    error: e.to_string(),
                });
                failed_count += 1;
            }
        }
    }

    tracing::info!(
        synced_count,
        failed_count,
        "Completed sync of archived tasks to Hive"
    );

    Ok(ResponseJson(ApiResponse::success(
        SyncArchivedToHiveResponse {
            synced_count,
            failed_count,
            errors,
        },
    )))
}
