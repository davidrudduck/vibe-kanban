//! Label management handlers: get_task_labels, set_task_labels.

use axum::{Extension, Json, extract::State, response::Json as ResponseJson};
use db::models::{
    label::{Label, SetTaskLabels},
    task::Task,
};
use deployment::Deployment;
use utils::response::ApiResponse;

use super::remote::resync_task_to_hive;
use crate::{DeploymentImpl, error::ApiError};

// ============================================================================
// Get Labels
// ============================================================================

/// GET /api/tasks/{id}/labels - Get labels for a task
pub async fn get_task_labels(
    Extension(task): Extension<Task>,
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<Vec<Label>>>, ApiError> {
    let labels = Label::find_by_task_id(&deployment.db().pool, task.id).await?;
    Ok(ResponseJson(ApiResponse::success(labels)))
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
