//! Label management handlers: get_task_labels, set_task_labels.

use axum::{Extension, Json, extract::State, response::Json as ResponseJson};
use db::models::{
    label::{Label, SetTaskLabels},
    task::Task,
};
use deployment::Deployment;
use utils::response::ApiResponse;

use super::remote::resync_task_to_hive;
use crate::middleware::RemoteTaskNeeded;
use crate::{DeploymentImpl, error::ApiError};

// ============================================================================
// Get Labels
// ============================================================================

/// GET /api/tasks/{id}/labels - Get labels for a task
///
/// Supports both local tasks and remote tasks fetched from Hive.
pub async fn get_task_labels(
    local_task: Option<Extension<Task>>,
    remote_needed: Option<Extension<RemoteTaskNeeded>>,
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<Vec<Label>>>, ApiError> {
    // Local task - fetch labels from local DB
    if let Some(Extension(task)) = local_task {
        let labels = Label::find_by_task_id(&deployment.db().pool, task.id).await?;
        return Ok(ResponseJson(ApiResponse::success(labels)));
    }

    // Remote task - fetch from Hive
    if let Some(Extension(remote)) = remote_needed {
        // Use OAuth client only - the labels endpoint requires OAuth auth
        // (unlike get_shared_task which has a separate /v1/sync/swarm/tasks/{id} endpoint)
        let client = deployment.remote_client().map_err(|e| {
            tracing::warn!(
                task_id = %remote.task_id,
                error = %e,
                "No OAuth client available for labels lookup"
            );
            ApiError::BadGateway("No OAuth client available for labels".into())
        })?;

        match client.get_task_labels(remote.task_id).await {
            Ok(response) => {
                // Convert remote labels to local Label format
                let labels: Vec<Label> = response
                    .labels
                    .into_iter()
                    .map(|remote_label| Label {
                        id: remote_label.id,
                        project_id: remote_label.project_id,
                        name: remote_label.name,
                        icon: remote_label.icon,
                        color: remote_label.color,
                        shared_label_id: Some(remote_label.id), // The remote ID is the shared ID
                        version: remote_label.version,
                        synced_at: None, // Not synced locally
                        created_at: remote_label.created_at,
                        updated_at: remote_label.updated_at,
                    })
                    .collect();
                return Ok(ResponseJson(ApiResponse::success(labels)));
            }
            Err(e) if e.is_not_found() => {
                tracing::debug!(
                    task_id = %remote.task_id,
                    "Task not found on Hive"
                );
                return Err(ApiError::NotFound("Task not found".into()));
            }
            Err(e) => {
                tracing::warn!(
                    task_id = %remote.task_id,
                    error = %e,
                    "Failed to fetch labels from Hive"
                );
                return Err(e.into());
            }
        }
    }

    // Neither local nor remote task found
    Err(ApiError::NotFound("Task not found".into()))
}

// ============================================================================
// Set Labels
// ============================================================================

/// PUT /api/tasks/{id}/labels - Set labels for a task (replaces existing)
pub async fn set_task_labels(
    Extension(task): Extension<Task>,
    State(deployment): State<DeploymentImpl>,
    Json(payload): Json<SetTaskLabels>,
) -> Result<ResponseJson<ApiResponse<Vec<Label>>>, ApiError> {
    // For tasks synced from Hive, proxy to Hive labels API
    if let Some(shared_task_id) = task.shared_task_id {
        let remote_client = deployment.remote_client()?;

        match remote_client
            .set_task_labels(shared_task_id, &payload.label_ids)
            .await
        {
            Ok(_response) => {
                // Labels set on Hive - return empty vec since we don't sync Hive labels locally
                return Ok(ResponseJson(ApiResponse::success(vec![])));
            }
            Err(e) if e.is_not_found() => {
                // Task doesn't exist on Hive - resync first, then retry labels
                tracing::warn!(
                    task_id = %task.id,
                    shared_task_id = %shared_task_id,
                    "Shared task not found on Hive during label update, re-syncing"
                );

                let resynced_task =
                    resync_task_to_hive(&deployment, &task, None, None, None).await?;

                // Retry setting labels with the new shared_task_id
                if let Some(new_shared_task_id) = resynced_task.shared_task_id {
                    remote_client
                        .set_task_labels(new_shared_task_id, &payload.label_ids)
                        .await?;
                }

                return Ok(ResponseJson(ApiResponse::success(vec![])));
            }
            Err(e) => return Err(e.into()),
        }
    }

    // Local task: use local labels
    let labels = Label::set_task_labels(&deployment.db().pool, task.id, &payload.label_ids).await?;
    Ok(ResponseJson(ApiResponse::success(labels)))
}
