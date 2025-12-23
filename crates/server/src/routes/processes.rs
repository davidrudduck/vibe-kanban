//! API routes for vibe-kanban process management.
//!
//! Provides endpoints for listing and killing processes spawned by vibe-kanban
//! task executions.

use axum::{
    Json, Router,
    extract::{Query, State},
    response::Json as ResponseJson,
    routing::{get, post},
};
use deployment::Deployment;
use serde::Deserialize;
use services::services::{
    process_inspector::SysinfoProcessInspector,
    process_service::{KillResult, KillScope, ProcessFilter, ProcessInfo, ProcessService},
};
use utils::response::ApiResponse;
use uuid::Uuid;

use crate::{DeploymentImpl, error::ApiError};

/// Query parameters for listing processes.
#[derive(Debug, Deserialize)]
pub struct ListProcessesQuery {
    /// Filter by project ID
    pub project_id: Option<Uuid>,
    /// Filter by task ID
    pub task_id: Option<Uuid>,
    /// Filter by task attempt ID
    pub task_attempt_id: Option<Uuid>,
    /// Only include executor processes (exclude children)
    #[serde(default)]
    pub executors_only: bool,
}

impl From<ListProcessesQuery> for ProcessFilter {
    fn from(query: ListProcessesQuery) -> Self {
        Self {
            project_id: query.project_id,
            task_id: query.task_id,
            task_attempt_id: query.task_attempt_id,
            executors_only: query.executors_only,
        }
    }
}

/// Request body for kill operations.
#[derive(Debug, Deserialize)]
pub struct KillProcessesRequest {
    /// The scope of processes to kill
    pub scope: KillScope,
    /// Whether to force kill (SIGKILL vs SIGTERM)
    #[serde(default)]
    pub force: bool,
}

/// List all vibe-kanban related processes.
///
/// GET /api/processes
/// GET /api/processes?project_id=X
/// GET /api/processes?task_id=X
/// GET /api/processes?task_attempt_id=X
/// GET /api/processes?executors_only=true
pub async fn list_processes(
    State(deployment): State<DeploymentImpl>,
    Query(query): Query<ListProcessesQuery>,
) -> Result<ResponseJson<ApiResponse<Vec<ProcessInfo>>>, ApiError> {
    let service = ProcessService::new(SysinfoProcessInspector::new());
    let filter = ProcessFilter::from(query);

    let processes = service
        .list_processes(&deployment.db().pool, Some(filter))
        .await?;

    Ok(ResponseJson(ApiResponse::success(processes)))
}

/// Kill processes by scope.
///
/// POST /api/processes/kill
/// Body: { "scope": { "type": "single", "pid": 1234 } }
/// Body: { "scope": { "type": "task", "task_id": "uuid" } }
/// Body: { "scope": { "type": "project", "project_id": "uuid" } }
/// Body: { "scope": { "type": "all_except_executors" } }
/// Body: { "scope": { "type": "all" } }
pub async fn kill_processes(
    State(deployment): State<DeploymentImpl>,
    Json(request): Json<KillProcessesRequest>,
) -> Result<ResponseJson<ApiResponse<KillResult>>, ApiError> {
    let service = ProcessService::new(SysinfoProcessInspector::new());

    let result = service
        .kill_processes(&deployment.db().pool, request.scope, request.force)
        .await?;

    Ok(ResponseJson(ApiResponse::success(result)))
}

/// Create the router for process management endpoints.
pub fn router(deployment: &DeploymentImpl) -> Router<DeploymentImpl> {
    let _ = deployment; // Unused but kept for consistency with other routers

    Router::new()
        .route("/processes", get(list_processes))
        .route("/processes/kill", post(kill_processes))
}
