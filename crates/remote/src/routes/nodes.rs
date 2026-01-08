use axum::{
    Json, Router,
    extract::{Extension, Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{delete, get, post},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::{Span, instrument};
use uuid::Uuid;

use super::organization_members::ensure_member_access;
use crate::{
    AppState,
    auth::RequestContext,
    db::organizations::{MemberRole, OrganizationRepository},
    nodes::{
        CreateNodeApiKey, HeartbeatPayload, LinkProjectData, MergeNodesResult, Node, NodeApiKey,
        NodeError, NodeExecutionProcess, NodeProject, NodeRegistration, NodeServiceImpl,
        NodeTaskAttempt,
    },
};

/// Header name for API key authentication
const API_KEY_HEADER: &str = "x-api-key";

// ============================================================================
// Router Setup
// ============================================================================

/// Routes that require API key authentication (for nodes)
pub fn api_key_router() -> Router<AppState> {
    Router::new()
        .route("/nodes/register", post(register_node))
        .route("/nodes/{node_id}/heartbeat", post(heartbeat))
        .route("/nodes/{node_id}/projects", post(link_project))
        .route(
            "/nodes/{node_id}/projects/{link_id}",
            delete(unlink_project_by_id),
        )
}

/// Routes that require user JWT authentication
pub fn protected_router() -> Router<AppState> {
    Router::new()
        .route("/nodes/api-keys", post(create_api_key))
        .route("/nodes/api-keys", get(list_api_keys))
        .route("/nodes/api-keys/{key_id}", delete(revoke_api_key))
        .route("/nodes/api-keys/{key_id}/unblock", post(unblock_api_key))
        .route("/nodes", get(list_nodes))
        .route("/nodes/{node_id}", get(get_node))
        .route("/nodes/{node_id}", delete(delete_node))
        .route("/nodes/{node_id}/projects", get(list_node_projects))
        .route(
            "/nodes/{node_id}/projects/linked",
            get(list_linked_node_projects),
        )
        .route("/nodes/{source_id}/merge-to/{target_id}", post(merge_nodes))
        .route(
            "/nodes/assignments/{assignment_id}/logs",
            get(get_assignment_logs),
        )
        .route(
            "/nodes/assignments/{assignment_id}/progress",
            get(get_assignment_progress),
        )
        .route(
            "/nodes/assignments/{assignment_id}/connection-info",
            get(get_connection_info),
        )
        .route(
            "/nodes/task-attempts/by-shared-task/{shared_task_id}",
            get(list_task_attempts_by_shared_task),
        )
        .route(
            "/nodes/task-attempts/{attempt_id}",
            get(get_node_task_attempt),
        )
}

// ============================================================================
// API Key Management (User JWT Auth)
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct CreateApiKeyRequest {
    pub organization_id: Uuid,
    pub name: String,
}

#[derive(Debug, Serialize)]
pub struct CreateApiKeyResponse {
    pub api_key: NodeApiKey,
    /// The raw API key value - only returned on creation
    pub secret: String,
}

#[instrument(
    name = "nodes.create_api_key",
    skip(state, ctx, payload),
    fields(user_id = %ctx.user.id, org_id = %payload.organization_id)
)]
pub async fn create_api_key(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
    Json(payload): Json<CreateApiKeyRequest>,
) -> Response {
    let pool = state.pool();

    // Verify user has access to organization
    if let Err(error) = ensure_member_access(pool, payload.organization_id, ctx.user.id).await {
        return error.into_response();
    }

    let service = NodeServiceImpl::new(pool.clone());
    let data = CreateNodeApiKey { name: payload.name };

    match service
        .create_api_key(payload.organization_id, data, ctx.user.id)
        .await
    {
        Ok((api_key, secret)) => (
            StatusCode::CREATED,
            Json(CreateApiKeyResponse { api_key, secret }),
        )
            .into_response(),
        Err(error) => node_error_response(error, "failed to create API key"),
    }
}

#[derive(Debug, Deserialize)]
pub struct ListApiKeysQuery {
    pub organization_id: Uuid,
}

#[instrument(
    name = "nodes.list_api_keys",
    skip(state, ctx, query),
    fields(user_id = %ctx.user.id, org_id = %query.organization_id)
)]
pub async fn list_api_keys(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
    Query(query): Query<ListApiKeysQuery>,
) -> Response {
    let pool = state.pool();

    if let Err(error) = ensure_member_access(pool, query.organization_id, ctx.user.id).await {
        return error.into_response();
    }

    let service = NodeServiceImpl::new(pool.clone());

    match service.list_api_keys(query.organization_id).await {
        Ok(keys) => (StatusCode::OK, Json(keys)).into_response(),
        Err(error) => node_error_response(error, "failed to list API keys"),
    }
}

#[instrument(
    name = "nodes.revoke_api_key",
    skip(state, ctx),
    fields(user_id = %ctx.user.id, key_id = %key_id)
)]
pub async fn revoke_api_key(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
    Path(key_id): Path<Uuid>,
) -> Response {
    let pool = state.pool();
    let service = NodeServiceImpl::new(pool.clone());

    // TODO: Verify the key belongs to an organization the user has access to
    let _ = ctx; // Silence unused warning for now

    match service.revoke_api_key(key_id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => node_error_response(error, "failed to revoke API key"),
    }
}

// ============================================================================
// Node Registration (API Key Auth)
// ============================================================================

#[derive(Debug, Serialize)]
pub struct RegisterNodeResponse {
    pub node: Node,
    pub linked_projects: Vec<NodeProject>,
}

#[instrument(
    name = "nodes.register",
    skip(state, headers, payload),
    fields(machine_id = %payload.machine_id)
)]
pub async fn register_node(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<NodeRegistration>,
) -> Response {
    let pool = state.pool();
    let service = NodeServiceImpl::new(pool.clone());

    // Validate API key
    let api_key = match extract_and_validate_api_key(&service, &headers).await {
        Ok(key) => key,
        Err(response) => return response,
    };

    Span::current().record("org_id", format_args!("{}", api_key.organization_id));

    // Register or update node
    match service
        .register_node(api_key.organization_id, payload)
        .await
    {
        Ok(node) => {
            // Get linked projects
            let linked_projects = service
                .list_node_projects(node.id)
                .await
                .unwrap_or_default();

            (
                StatusCode::OK,
                Json(RegisterNodeResponse {
                    node,
                    linked_projects,
                }),
            )
                .into_response()
        }
        Err(error) => node_error_response(error, "failed to register node"),
    }
}

#[instrument(
    name = "nodes.heartbeat",
    skip(state, headers, payload),
    fields(node_id = %node_id)
)]
pub async fn heartbeat(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(node_id): Path<Uuid>,
    Json(payload): Json<HeartbeatPayload>,
) -> Response {
    let pool = state.pool();
    let service = NodeServiceImpl::new(pool.clone());

    // Validate API key
    if let Err(response) = extract_and_validate_api_key(&service, &headers).await {
        return response;
    }

    // TODO: Verify node belongs to the organization from the API key

    match service.heartbeat(node_id, payload).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => node_error_response(error, "failed to process heartbeat"),
    }
}

// ============================================================================
// Project Linking (API Key Auth)
// ============================================================================

#[instrument(
    name = "nodes.link_project",
    skip(state, headers, payload),
    fields(node_id = %node_id, project_id = %payload.project_id)
)]
pub async fn link_project(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(node_id): Path<Uuid>,
    Json(payload): Json<LinkProjectData>,
) -> Response {
    let pool = state.pool();
    let service = NodeServiceImpl::new(pool.clone());

    // Validate API key
    if let Err(response) = extract_and_validate_api_key(&service, &headers).await {
        return response;
    }

    match service.link_project(node_id, payload).await {
        Ok(link) => (StatusCode::CREATED, Json(link)).into_response(),
        Err(error) => node_error_response(error, "failed to link project"),
    }
}

#[instrument(
    name = "nodes.unlink_project",
    skip(state, headers),
    fields(node_id = %node_id, link_id = %link_id)
)]
pub async fn unlink_project_by_id(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((node_id, link_id)): Path<(Uuid, Uuid)>,
) -> Response {
    let pool = state.pool();
    let service = NodeServiceImpl::new(pool.clone());

    // Validate API key
    if let Err(response) = extract_and_validate_api_key(&service, &headers).await {
        return response;
    }

    let _ = node_id; // We could verify the link belongs to this node

    // For now, just delete by project_id from the link
    // In the future we should verify the link belongs to the node
    match service.unlink_project(link_id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => node_error_response(error, "failed to unlink project"),
    }
}

// ============================================================================
// Node Management (User JWT Auth)
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct ListNodesQuery {
    pub organization_id: Uuid,
}

#[instrument(
    name = "nodes.list",
    skip(state, ctx, query),
    fields(user_id = %ctx.user.id, org_id = %query.organization_id)
)]
pub async fn list_nodes(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
    Query(query): Query<ListNodesQuery>,
) -> Response {
    let pool = state.pool();

    if let Err(error) = ensure_member_access(pool, query.organization_id, ctx.user.id).await {
        return error.into_response();
    }

    let service = NodeServiceImpl::new(pool.clone());

    match service.list_nodes(query.organization_id).await {
        Ok(nodes) => (StatusCode::OK, Json(nodes)).into_response(),
        Err(error) => node_error_response(error, "failed to list nodes"),
    }
}

#[instrument(
    name = "nodes.get",
    skip(state, ctx),
    fields(user_id = %ctx.user.id, node_id = %node_id)
)]
pub async fn get_node(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
    Path(node_id): Path<Uuid>,
) -> Response {
    let pool = state.pool();
    let service = NodeServiceImpl::new(pool.clone());

    let _ = ctx; // TODO: Verify user has access to the node's organization

    match service.get_node(node_id).await {
        Ok(node) => (StatusCode::OK, Json(node)).into_response(),
        Err(error) => node_error_response(error, "failed to get node"),
    }
}

#[instrument(
    name = "nodes.delete",
    skip(state, ctx),
    fields(user_id = %ctx.user.id, node_id = %node_id)
)]
pub async fn delete_node(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
    Path(node_id): Path<Uuid>,
) -> Response {
    let pool = state.pool();
    let service = NodeServiceImpl::new(pool.clone());

    // Get node to verify organization access
    let node = match service.get_node(node_id).await {
        Ok(node) => node,
        Err(NodeError::NodeNotFound) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Node not found" })),
            )
                .into_response();
        }
        Err(error) => return node_error_response(error, "failed to get node"),
    };

    // Verify user is admin of the node's organization
    let org_repo = OrganizationRepository::new(pool);
    match org_repo
        .check_user_role(node.organization_id, ctx.user.id)
        .await
    {
        Ok(Some(MemberRole::Admin)) => {}
        Ok(_) => {
            return (
                StatusCode::FORBIDDEN,
                Json(json!({ "error": "Admin access required to delete nodes" })),
            )
                .into_response();
        }
        Err(_) => {
            return (
                StatusCode::FORBIDDEN,
                Json(json!({ "error": "Admin access required to delete nodes" })),
            )
                .into_response();
        }
    }

    // Delete the node (cascades node_projects, task_assignments)
    match service.delete_node(node_id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => node_error_response(error, "failed to delete node"),
    }
}

#[instrument(
    name = "nodes.list_projects",
    skip(state, ctx),
    fields(user_id = %ctx.user.id, node_id = %node_id)
)]
pub async fn list_node_projects(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
    Path(node_id): Path<Uuid>,
) -> Response {
    let pool = state.pool();
    let service = NodeServiceImpl::new(pool.clone());

    let _ = ctx; // TODO: Verify user has access to the node's organization

    // Return local projects with swarm project info for the settings UI
    match service.list_node_local_projects(node_id).await {
        Ok(projects) => (StatusCode::OK, Json(projects)).into_response(),
        Err(error) => node_error_response(error, "failed to list node projects"),
    }
}

/// List projects linked to a node that are also linked to a swarm project.
/// Only swarm-linked projects are returned - unlinked projects are excluded.
/// Use this endpoint for syncing projects to other nodes.
#[instrument(
    name = "nodes.list_linked_projects",
    skip(state, ctx),
    fields(user_id = %ctx.user.id, node_id = %node_id)
)]
pub async fn list_linked_node_projects(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
    Path(node_id): Path<Uuid>,
) -> Response {
    let pool = state.pool();
    let service = NodeServiceImpl::new(pool.clone());

    let _ = ctx; // TODO: Verify user has access to the node's organization

    match service.list_linked_node_projects(node_id).await {
        Ok(projects) => (StatusCode::OK, Json(projects)).into_response(),
        Err(error) => node_error_response(error, "failed to list linked node projects"),
    }
}

// ============================================================================
// Node Merge (User JWT Auth - Admin Only)
// ============================================================================

#[derive(Debug, Serialize)]
pub struct MergeNodesResponse {
    pub source_node_id: Uuid,
    pub target_node_id: Uuid,
    pub projects_moved: u64,
    pub keys_rebound: u64,
}

impl From<MergeNodesResult> for MergeNodesResponse {
    fn from(result: MergeNodesResult) -> Self {
        Self {
            source_node_id: result.source_node_id,
            target_node_id: result.target_node_id,
            projects_moved: result.projects_moved,
            keys_rebound: result.keys_rebound,
        }
    }
}

#[instrument(
    name = "nodes.merge",
    skip(state, ctx),
    fields(user_id = %ctx.user.id, source_id = %source_id, target_id = %target_id)
)]
pub async fn merge_nodes(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
    Path((source_id, target_id)): Path<(Uuid, Uuid)>,
) -> Response {
    let pool = state.pool();
    let service = NodeServiceImpl::new(pool.clone());

    // Get source node to verify organization access
    let source_node = match service.get_node(source_id).await {
        Ok(node) => node,
        Err(NodeError::NodeNotFound) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Source node not found" })),
            )
                .into_response();
        }
        Err(error) => return node_error_response(error, "failed to get source node"),
    };

    // Verify user is admin of the node's organization
    let org_repo = OrganizationRepository::new(pool);
    match org_repo
        .check_user_role(source_node.organization_id, ctx.user.id)
        .await
    {
        Ok(Some(MemberRole::Admin)) => {}
        Ok(_) => {
            return (
                StatusCode::FORBIDDEN,
                Json(json!({ "error": "Admin access required to merge nodes" })),
            )
                .into_response();
        }
        Err(_) => {
            return (
                StatusCode::FORBIDDEN,
                Json(json!({ "error": "Admin access required to merge nodes" })),
            )
                .into_response();
        }
    }

    // Perform the merge
    match service.merge_nodes(source_id, target_id).await {
        Ok(result) => (StatusCode::OK, Json(MergeNodesResponse::from(result))).into_response(),
        Err(error) => node_error_response(error, "failed to merge nodes"),
    }
}

// ============================================================================
// Unblock API Key (User JWT Auth - Admin Only)
// ============================================================================

#[instrument(
    name = "nodes.unblock_api_key",
    skip(state, ctx),
    fields(user_id = %ctx.user.id, key_id = %key_id)
)]
pub async fn unblock_api_key(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
    Path(key_id): Path<Uuid>,
) -> Response {
    use crate::db::node_api_keys::NodeApiKeyRepository;

    let pool = state.pool();
    let service = NodeServiceImpl::new(pool.clone());

    // Get the API key to verify organization access
    let key_repo = NodeApiKeyRepository::new(pool);
    let api_key = match key_repo.find_by_id(key_id).await {
        Ok(Some(key)) => key,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "API key not found" })),
            )
                .into_response();
        }
        Err(e) => {
            tracing::error!(?e, "failed to fetch API key");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "internal server error" })),
            )
                .into_response();
        }
    };

    // Verify user is admin of the API key's organization
    let org_repo = OrganizationRepository::new(pool);
    match org_repo
        .check_user_role(api_key.organization_id, ctx.user.id)
        .await
    {
        Ok(Some(MemberRole::Admin)) => {}
        Ok(_) => {
            return (
                StatusCode::FORBIDDEN,
                Json(json!({ "error": "Admin access required to unblock API keys" })),
            )
                .into_response();
        }
        Err(_) => {
            return (
                StatusCode::FORBIDDEN,
                Json(json!({ "error": "Admin access required to unblock API keys" })),
            )
                .into_response();
        }
    }

    // Unblock the key
    match service.unblock_api_key(key_id).await {
        Ok(key) => (StatusCode::OK, Json(key)).into_response(),
        Err(error) => node_error_response(error, "failed to unblock API key"),
    }
}

// ============================================================================
// Task Output Logs (User JWT Auth)
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct GetAssignmentLogsQuery {
    /// Maximum number of logs to return
    pub limit: Option<i64>,
    /// Return logs after this ID (for pagination)
    pub after_id: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct TaskOutputLogResponse {
    pub id: i64,
    pub assignment_id: Uuid,
    pub output_type: String,
    pub content: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize)]
pub struct GetAssignmentLogsResponse {
    pub logs: Vec<TaskOutputLogResponse>,
}

#[instrument(
    name = "nodes.get_assignment_logs",
    skip(state, ctx, query),
    fields(user_id = %ctx.user.id, assignment_id = %assignment_id)
)]
pub async fn get_assignment_logs(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
    Path(assignment_id): Path<Uuid>,
    Query(query): Query<GetAssignmentLogsQuery>,
) -> Response {
    use crate::db::task_assignments::TaskAssignmentRepository;
    use crate::db::task_output_logs::TaskOutputLogRepository;

    let pool = state.pool();

    // Get the assignment to verify access
    let assignment_repo = TaskAssignmentRepository::new(pool);
    let assignment = match assignment_repo.find_by_id(assignment_id).await {
        Ok(Some(a)) => a,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "assignment not found" })),
            )
                .into_response();
        }
        Err(e) => {
            tracing::error!(?e, "failed to fetch assignment");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "internal server error" })),
            )
                .into_response();
        }
    };

    // Get the task to verify organization access
    use crate::db::tasks::SharedTaskRepository;
    let task_repo = SharedTaskRepository::new(pool);
    let task = match task_repo.find_by_id(assignment.task_id).await {
        Ok(Some(t)) => t,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "task not found" })),
            )
                .into_response();
        }
        Err(e) => {
            tracing::error!(?e, "failed to fetch task");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "internal server error" })),
            )
                .into_response();
        }
    };

    // Verify user has access to the organization
    if let Err(error) = ensure_member_access(pool, task.organization_id, ctx.user.id).await {
        return error.into_response();
    }

    // Get the logs
    let log_repo = TaskOutputLogRepository::new(pool);
    match log_repo
        .list_by_assignment(assignment_id, query.limit, query.after_id)
        .await
    {
        Ok(logs) => {
            let response = GetAssignmentLogsResponse {
                logs: logs
                    .into_iter()
                    .map(|log| TaskOutputLogResponse {
                        id: log.id,
                        assignment_id: log.assignment_id,
                        output_type: log.output_type,
                        content: log.content,
                        timestamp: log.timestamp,
                        created_at: log.created_at,
                    })
                    .collect(),
            };
            (StatusCode::OK, Json(response)).into_response()
        }
        Err(e) => {
            tracing::error!(?e, "failed to fetch assignment logs");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "internal server error" })),
            )
                .into_response()
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct GetAssignmentProgressQuery {
    /// Maximum number of events to return
    pub limit: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct TaskProgressEventResponse {
    pub id: i64,
    pub assignment_id: Uuid,
    pub event_type: String,
    pub message: Option<String>,
    pub metadata: Option<serde_json::Value>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize)]
pub struct GetAssignmentProgressResponse {
    pub events: Vec<TaskProgressEventResponse>,
}

#[instrument(
    name = "nodes.get_assignment_progress",
    skip(state, ctx, query),
    fields(user_id = %ctx.user.id, assignment_id = %assignment_id)
)]
pub async fn get_assignment_progress(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
    Path(assignment_id): Path<Uuid>,
    Query(query): Query<GetAssignmentProgressQuery>,
) -> Response {
    use crate::db::task_assignments::TaskAssignmentRepository;
    use crate::db::task_progress_events::TaskProgressEventRepository;

    let pool = state.pool();

    // Get the assignment to verify access
    let assignment_repo = TaskAssignmentRepository::new(pool);
    let assignment = match assignment_repo.find_by_id(assignment_id).await {
        Ok(Some(a)) => a,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "assignment not found" })),
            )
                .into_response();
        }
        Err(e) => {
            tracing::error!(?e, "failed to fetch assignment");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "internal server error" })),
            )
                .into_response();
        }
    };

    // Get the task to verify organization access
    use crate::db::tasks::SharedTaskRepository;
    let task_repo = SharedTaskRepository::new(pool);
    let task = match task_repo.find_by_id(assignment.task_id).await {
        Ok(Some(t)) => t,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "task not found" })),
            )
                .into_response();
        }
        Err(e) => {
            tracing::error!(?e, "failed to fetch task");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "internal server error" })),
            )
                .into_response();
        }
    };

    // Verify user has access to the organization
    if let Err(error) = ensure_member_access(pool, task.organization_id, ctx.user.id).await {
        return error.into_response();
    }

    // Get the progress events
    let progress_repo = TaskProgressEventRepository::new(pool);
    match progress_repo
        .list_by_assignment(assignment_id, query.limit)
        .await
    {
        Ok(events) => {
            let response = GetAssignmentProgressResponse {
                events: events
                    .into_iter()
                    .map(|event| TaskProgressEventResponse {
                        id: event.id,
                        assignment_id: event.assignment_id,
                        event_type: event.event_type,
                        message: event.message,
                        metadata: event.metadata,
                        timestamp: event.timestamp,
                        created_at: event.created_at,
                    })
                    .collect(),
            };
            (StatusCode::OK, Json(response)).into_response()
        }
        Err(e) => {
            tracing::error!(?e, "failed to fetch assignment progress events");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "internal server error" })),
            )
                .into_response()
        }
    }
}

// ============================================================================
// Connection Info (User JWT Auth)
// ============================================================================

/// Response containing connection information for streaming logs from a node.
#[derive(Debug, Serialize)]
pub struct ConnectionInfoResponse {
    /// The assignment ID
    pub assignment_id: Uuid,
    /// The node ID
    pub node_id: Uuid,
    /// Direct URL to the node (if available)
    pub direct_url: Option<String>,
    /// Hive relay URL for log streaming
    pub relay_url: String,
    /// Short-lived token for authenticating with the node or relay
    pub connection_token: String,
    /// Token expiration timestamp (ISO 8601)
    pub expires_at: String,
}

#[instrument(
    name = "nodes.get_connection_info",
    skip(state, ctx),
    fields(user_id = %ctx.user.id, assignment_id = %assignment_id)
)]
pub async fn get_connection_info(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
    Path(assignment_id): Path<Uuid>,
) -> Response {
    use crate::db::nodes::NodeRepository;
    use crate::db::task_assignments::TaskAssignmentRepository;

    let pool = state.pool();

    // Get the assignment
    let assignment_repo = TaskAssignmentRepository::new(pool);
    let assignment = match assignment_repo.find_by_id(assignment_id).await {
        Ok(Some(a)) => a,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "assignment not found" })),
            )
                .into_response();
        }
        Err(e) => {
            tracing::error!(?e, "failed to fetch assignment");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "internal server error" })),
            )
                .into_response();
        }
    };

    // Get the task to verify organization access
    use crate::db::tasks::SharedTaskRepository;
    let task_repo = SharedTaskRepository::new(pool);
    let task = match task_repo.find_by_id(assignment.task_id).await {
        Ok(Some(t)) => t,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "task not found" })),
            )
                .into_response();
        }
        Err(e) => {
            tracing::error!(?e, "failed to fetch task");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "internal server error" })),
            )
                .into_response();
        }
    };

    // Verify user has access to the organization
    if let Err(error) = ensure_member_access(pool, task.organization_id, ctx.user.id).await {
        return error.into_response();
    }

    // Get the node to get its public URL
    let node_repo = NodeRepository::new(pool);
    let node = match node_repo.find_by_id(assignment.node_id).await {
        Ok(Some(n)) => n,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "node not found" })),
            )
                .into_response();
        }
        Err(e) => {
            tracing::error!(?e, "failed to fetch node");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "internal server error" })),
            )
                .into_response();
        }
    };

    // Generate connection token
    let connection_token_service = state.connection_token();
    let token = match connection_token_service.generate(
        ctx.user.id,
        node.id,
        assignment_id,
        assignment.local_attempt_id,
    ) {
        Ok(t) => t,
        Err(e) => {
            tracing::error!(?e, "failed to generate connection token");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "internal server error" })),
            )
                .into_response();
        }
    };

    // Calculate expiration time (15 minutes from now)
    let expires_at = chrono::Utc::now() + chrono::Duration::minutes(15);

    // Build relay URL
    let relay_url = format!(
        "{}/v1/nodes/assignments/{}/logs/ws",
        state.server_public_base_url, assignment_id
    );

    let response = ConnectionInfoResponse {
        assignment_id,
        node_id: node.id,
        direct_url: node.public_url,
        relay_url,
        connection_token: token,
        expires_at: expires_at.to_rfc3339(),
    };

    (StatusCode::OK, Json(response)).into_response()
}

// ============================================================================
// Task Attempts (User JWT Auth)
// ============================================================================

#[derive(Debug, Serialize)]
pub struct ListTaskAttemptsBySharedTaskResponse {
    pub attempts: Vec<NodeTaskAttempt>,
}

/// List all task attempts for a shared task.
/// Used by remote nodes to fetch attempts for swarm tasks via the Hive.
#[instrument(
    name = "nodes.list_task_attempts_by_shared_task",
    skip(state, ctx),
    fields(user_id = %ctx.user.id, shared_task_id = %shared_task_id)
)]
pub async fn list_task_attempts_by_shared_task(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
    Path(shared_task_id): Path<Uuid>,
) -> Response {
    let pool = state.pool();

    // Get the shared task to verify organization access
    use crate::db::tasks::SharedTaskRepository;
    let task_repo = SharedTaskRepository::new(pool);
    let task = match task_repo.find_by_id(shared_task_id).await {
        Ok(Some(t)) => t,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "shared task not found" })),
            )
                .into_response();
        }
        Err(e) => {
            tracing::error!(?e, "failed to fetch shared task");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "internal server error" })),
            )
                .into_response();
        }
    };

    // Verify user has access to the organization
    if let Err(error) = ensure_member_access(pool, task.organization_id, ctx.user.id).await {
        return error.into_response();
    }

    // Get the task attempts
    let service = NodeServiceImpl::new(pool.clone());
    match service.list_task_attempts_by_shared_task(shared_task_id).await {
        Ok(attempts) => {
            let response = ListTaskAttemptsBySharedTaskResponse { attempts };
            (StatusCode::OK, Json(response)).into_response()
        }
        Err(error) => node_error_response(error, "failed to list task attempts"),
    }
}

/// Response for getting a single node task attempt
#[derive(Debug, Serialize)]
pub struct NodeTaskAttemptResponse {
    pub attempt: NodeTaskAttempt,
    pub executions: Vec<NodeExecutionProcess>,
    /// Whether all executions are in a terminal state (complete/failed/killed)
    pub is_complete: bool,
}

/// Get a single task attempt by ID.
/// Used by remote nodes to fetch attempt details for cross-node viewing.
#[instrument(
    name = "nodes.get_node_task_attempt",
    skip(state, ctx),
    fields(user_id = %ctx.user.id, attempt_id = %attempt_id)
)]
pub async fn get_node_task_attempt(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
    Path(attempt_id): Path<Uuid>,
) -> Response {
    let pool = state.pool();

    // Get the attempt
    use crate::db::node_task_attempts::NodeTaskAttemptRepository;
    let attempt_repo = NodeTaskAttemptRepository::new(pool);
    let attempt = match attempt_repo.find_by_id(attempt_id).await {
        Ok(Some(a)) => a,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "task attempt not found" })),
            )
                .into_response();
        }
        Err(e) => {
            tracing::error!(?e, "failed to fetch task attempt");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "internal server error" })),
            )
                .into_response();
        }
    };

    // Get the shared task to verify organization access
    use crate::db::tasks::SharedTaskRepository;
    let task_repo = SharedTaskRepository::new(pool);
    let task = match task_repo.find_by_id(attempt.shared_task_id).await {
        Ok(Some(t)) => t,
        Ok(None) => {
            tracing::warn!(
                attempt_id = %attempt_id,
                shared_task_id = %attempt.shared_task_id,
                "attempt references non-existent shared task"
            );
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "shared task not found" })),
            )
                .into_response();
        }
        Err(e) => {
            tracing::error!(?e, "failed to fetch shared task");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "internal server error" })),
            )
                .into_response();
        }
    };

    // Verify user has access to the organization
    if let Err(error) = ensure_member_access(pool, task.organization_id, ctx.user.id).await {
        return error.into_response();
    }

    // Trigger on-demand backfill for partial sync attempts (non-blocking)
    // This ensures that subsequent requests will have complete data
    if attempt.sync_state == "partial" {
        let backfill = state.backfill().clone();
        let node_id = attempt.node_id;
        let attempt_id_for_backfill = attempt.id;

        // Non-blocking - spawn and forget
        tokio::spawn(async move {
            if let Err(e) = backfill
                .request_immediate_backfill(node_id, attempt_id_for_backfill)
                .await
            {
                // Log at debug level since node being offline is expected
                tracing::debug!(
                    node_id = %node_id,
                    attempt_id = %attempt_id_for_backfill,
                    error = %e,
                    "on-demand backfill request failed (node may be offline)"
                );
            }
        });
    }

    // Get the execution processes for this attempt
    use crate::db::node_execution_processes::NodeExecutionProcessRepository;
    let exec_repo = NodeExecutionProcessRepository::new(pool);
    let executions = match exec_repo.find_by_attempt_id(attempt_id).await {
        Ok(execs) => execs,
        Err(e) => {
            tracing::error!(?e, "failed to fetch execution processes");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "internal server error" })),
            )
                .into_response();
        }
    };

    // Determine if the attempt is complete (all executions in terminal state)
    let is_complete = executions.iter().all(|e| {
        matches!(e.status.as_str(), "completed" | "failed" | "killed")
    });

    let response = NodeTaskAttemptResponse {
        attempt,
        executions,
        is_complete,
    };

    (StatusCode::OK, Json(response)).into_response()
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Extract and validate API key from headers
async fn extract_and_validate_api_key(
    service: &NodeServiceImpl,
    headers: &HeaderMap,
) -> Result<NodeApiKey, Response> {
    let api_key_value = headers
        .get(API_KEY_HEADER)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                Json(json!({ "error": "missing API key" })),
            )
                .into_response()
        })?;

    service.validate_api_key(api_key_value).await.map_err(|e| {
        let (status, message) = match e {
            NodeError::ApiKeyNotFound | NodeError::ApiKeyInvalid => {
                (StatusCode::UNAUTHORIZED, "invalid API key")
            }
            NodeError::ApiKeyRevoked => (StatusCode::UNAUTHORIZED, "API key revoked"),
            _ => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "failed to validate API key",
            ),
        };
        (status, Json(json!({ "error": message }))).into_response()
    })
}

/// Convert NodeError to HTTP response
fn node_error_response(error: NodeError, context: &str) -> Response {
    let (status, message): (StatusCode, String) = match &error {
        NodeError::NodeNotFound => (StatusCode::NOT_FOUND, "node not found".to_string()),
        NodeError::ApiKeyNotFound => (StatusCode::NOT_FOUND, "API key not found".to_string()),
        NodeError::ApiKeyInvalid => (StatusCode::UNAUTHORIZED, "invalid API key".to_string()),
        NodeError::ApiKeyRevoked => (StatusCode::UNAUTHORIZED, "API key revoked".to_string()),
        NodeError::ApiKeyBlocked(reason) => (
            StatusCode::FORBIDDEN,
            format!("API key blocked: {}", reason),
        ),
        NodeError::ApiKeyAlreadyBound => (
            StatusCode::CONFLICT,
            "API key already bound to a different node".to_string(),
        ),
        NodeError::TakeoverDetected(msg) => {
            (StatusCode::CONFLICT, format!("takeover detected: {}", msg))
        }
        NodeError::ProjectAlreadyLinked => (
            StatusCode::CONFLICT,
            "project already linked to a node".to_string(),
        ),
        NodeError::ProjectNotInHive => (
            StatusCode::BAD_REQUEST,
            "project not found in hive - sync project before linking".to_string(),
        ),
        NodeError::TaskAlreadyAssigned => (
            StatusCode::CONFLICT,
            "task already has an active assignment".to_string(),
        ),
        NodeError::AssignmentNotFound => {
            (StatusCode::NOT_FOUND, "assignment not found".to_string())
        }
        NodeError::NodeProjectNotFound => (
            StatusCode::NOT_FOUND,
            "node project link not found".to_string(),
        ),
        NodeError::Database(err) => {
            tracing::error!(?err, context, "database error in node operation");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal server error".to_string(),
            )
        }
    };

    (status, Json(json!({ "error": message }))).into_response()
}
