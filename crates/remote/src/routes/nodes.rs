use axum::{
    Json, Router,
    extract::{Extension, Path, Query, State},
    http::{HeaderMap, StatusCode},
    middleware,
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
    auth::{AuthContext, RequestContext, require_session_or_node_api_key},
    db::{
        organizations::{MemberRole, OrganizationRepository},
        swarm_projects::{SwarmProjectNode, SwarmProjectRepository},
    },
    nodes::{
        CreateNodeApiKey, HeartbeatPayload, MergeNodesResult, Node, NodeApiKey, NodeError,
        NodeExecutionProcess, NodeRegistration, NodeServiceImpl, NodeTaskAttempt,
    },
};

/// Header name for API key authentication
const API_KEY_HEADER: &str = "x-api-key";

// ============================================================================
// Router Setup
// ============================================================================

/// Creates the HTTP routes that require API key authentication for node operations.
///
/// # Examples
///
/// ```
/// let router = api_key_router();
/// // router now contains POST /nodes/register and POST /nodes/{node_id}/heartbeat
/// ```
pub fn api_key_router() -> Router<AppState> {
    Router::new()
        .route("/nodes/register", post(register_node))
        .route("/nodes/{node_id}/heartbeat", post(heartbeat))
        // Legacy project linking endpoints removed - use WebSocket LinkProject/UnlinkProject messages instead
}

/// Routes that require user JWT authentication
pub fn protected_router() -> Router<AppState> {
    Router::new()
        .route("/nodes/api-keys", post(create_api_key))
        .route("/nodes/api-keys", get(list_api_keys))
        .route("/nodes/api-keys/{key_id}", delete(revoke_api_key))
        .route("/nodes/api-keys/{key_id}/unblock", post(unblock_api_key))
        // Note: list_nodes moved to node_sync_router (accepts both JWT and API key)
        .route("/nodes/{node_id}", get(get_node))
        .route("/nodes/{node_id}", delete(delete_node))
        // Note: list_node_projects and list_linked_node_projects moved to node_sync_router
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

/// Create a router exposing node sync endpoints that accept either a user JWT or a node API key.
///
/// The router mounts endpoints used by nodes to synchronize projects and related data and applies
/// middleware that allows authentication via an active user session (JWT) or a valid node API key.
///
/// # Examples
///
/// ```
/// // Construct an application state (type must implement `Default` or provide an equivalent).
/// let state = AppState::default();
/// let router = node_sync_router(state);
/// // `router` now contains routes like `/nodes` and `/nodes/{node_id}/projects`.
/// ```
pub fn node_sync_router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/nodes", get(list_nodes_sync))
        .route("/nodes/{node_id}/projects", get(list_node_projects_sync))
        .route(
            "/nodes/{node_id}/projects/linked",
            get(list_linked_node_projects_sync),
        )
        .layer(middleware::from_fn_with_state(
            state,
            require_session_or_node_api_key,
        ))
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
    pub linked_projects: Vec<SwarmProjectNode>,
}

/// Registers or updates a node using the provided API key and node registration payload.
///
/// On success returns a 200 OK response containing the registered `Node` and any swarm projects
/// linked to that node. On failure returns an appropriate error response (authentication,
/// authorization, validation, or database errors).
///
/// # Examples
///
/// ```
/// # use axum::http::HeaderMap;
/// # use axum::extract::State;
/// # use axum::Json;
/// # use uuid::Uuid;
/// # use crate::routes::nodes::{register_node, NodeRegistration};
/// # // The following is illustrative and requires an application `AppState` and running runtime.
/// # #[tokio::test]
/// # async fn example_register_node_call() {
/// let state = /* obtain AppState */ todo!();
/// let mut headers = HeaderMap::new();
/// headers.insert("x-api-key", "example-key".parse().unwrap());
/// let payload = NodeRegistration {
///     machine_id: "machine-123".into(),
///     ..Default::default()
/// };
/// let response = register_node(State(state), headers, Json(payload)).await;
/// // Inspect `response` for status and body
/// # }
/// ```
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
            // Get linked swarm projects for this node
            let linked_projects = match SwarmProjectRepository::list_by_node(pool, node.id).await {
                Ok(projects) => projects,
                Err(e) => {
                    return node_error_response(
                        NodeError::Database(e.to_string()),
                        "failed to fetch linked projects for registered node",
                    );
                }
            };

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

/// Handle a node heartbeat request authenticated by an API key.
///
/// Validates the incoming API key from the `x-api-key` header, invokes the node service heartbeat
/// for the given `node_id` with the provided payload, and returns an HTTP response:
/// - 204 No Content on success
/// - a mapped error response on failure
///
/// # Examples
///
/// ```no_run
/// // Illustrative usage within an HTTP handler test environment:
/// // let resp = heartbeat(State(app_state), headers, Path(node_id), Json(payload)).await;
/// // assert_eq!(resp.status(), StatusCode::NO_CONTENT);
/// ```
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
// Node Management (User JWT Auth)
// Note: list_nodes, list_node_projects, list_linked_node_projects moved to
// Node Sync section below with dual-auth support (JWT or API key)
// ============================================================================

#[allow(dead_code)] // Superseded by ListNodesSyncQuery
#[derive(Debug, Deserialize)]
pub struct ListNodesQuery {
    pub organization_id: Uuid,
}

#[allow(dead_code)] // Superseded by list_nodes_sync
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

/// Delete the specified node if the requesting user is an organization admin.
///
/// Verifies the node exists, enforces that the requester has the Admin role for the node's
/// organization, and removes the node. Deletion cascades related `swarm_project_nodes` and
/// `task_assignments`.
///
/// # Returns
///
/// `204 No Content` on successful deletion; otherwise an appropriate HTTP error status with a
/// JSON error message (e.g., `404` if the node is not found, `403` if the requester lacks admin
/// access, `500` for internal errors).
///
/// # Examples
///
/// ```
/// // Intended to be mounted in an Axum router:
/// // router.delete("/nodes/:id", delete_node);
/// let _ = "delete_node handler";
/// ```
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

    // Delete the node (cascades swarm_project_nodes, task_assignments)
    match service.delete_node(node_id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => node_error_response(error, "failed to delete node"),
    }
}

#[allow(dead_code)] // Superseded by list_node_projects_sync
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
#[allow(dead_code)] // Superseded by list_linked_node_projects_sync
/// List projects that are linked to the specified node.
///
/// Returns an HTTP response containing a JSON array of linked swarm-project entries with status 200 on success,
/// or an error response mapped from internal node errors on failure.
///
/// # Examples
///
/// ```ignore
/// // Handler is intended to be used by the Axum router; this shows the expected shape of a request.
/// // The real handler is async and invoked by the web server with `State`, `Extension`, and `Path` extractors.
/// let response = list_linked_node_projects(state, ctx_extension, node_id).await;
/// // On success the response is a 200 JSON body containing the linked projects.
/// ```
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
// Node Sync (Dual Auth: User JWT or Node API Key)
// ============================================================================

/// Query parameters for listing nodes (sync version).
/// For node API key auth, organization_id is optional (uses API key's org).
#[derive(Debug, Deserialize)]
pub struct ListNodesSyncQuery {
    pub organization_id: Option<Uuid>,
}

/// List nodes for an organization supporting either user JWT or node API key authentication.
///
/// For user auth, the `organization_id` query parameter is required and the caller's membership
/// is verified. For node auth, the API key's organization is used if `organization_id` is omitted.
/// Returns HTTP 200 with the node list on success; returns an appropriate error response on failure.
///
/// # Examples
///
/// ```
/// use axum::extract::{State, Extension, Query};
/// use uuid::Uuid;
///
/// // Example: user-authenticated request (organization_id required)
/// let query = Query(ListNodesSyncQuery { organization_id: Some(Uuid::parse_str("00000000-0000-0000-0000-000000000000").unwrap()) });
/// // list_nodes_sync(State(app_state), Extension(auth_ctx_user), query).await;
///
/// // Example: node-authenticated request (organization_id optional)
/// let query_node = Query(ListNodesSyncQuery { organization_id: None });
/// // list_nodes_sync(State(app_state), Extension(auth_ctx_node), query_node).await;
/// ```
#[instrument(
name = "nodes.list_sync",
skip(state, auth_ctx, query),
)]
pub async fn list_nodes_sync(
    State(state): State<AppState>,
    Extension(auth_ctx): Extension<AuthContext>,
    Query(query): Query<ListNodesSyncQuery>,
) -> Response {
    let pool = state.pool();

    // Determine organization_id based on auth type
    let org_id = match &auth_ctx {
        AuthContext::User(user_ctx) => {
            // User auth: require organization_id query param
            let org_id = match query.organization_id {
                Some(id) => id,
                None => {
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(json!({ "error": "organization_id query parameter required" })),
                    )
                        .into_response();
                }
            };

            // Verify user has access to the organization
            if let Err(error) = ensure_member_access(pool, org_id, user_ctx.user.id).await {
                return error.into_response();
            }
            org_id
        }
        AuthContext::Node(node_ctx) => {
            // Node auth: use API key's organization
            // If organization_id is provided, verify it matches the API key's org
            match query.organization_id {
                Some(org_id) if org_id != node_ctx.organization_id => {
                    return (
                        StatusCode::FORBIDDEN,
                        Json(json!({ "error": "Cannot access nodes from different organization" })),
                    )
                        .into_response();
                }
                _ => node_ctx.organization_id,
            }
        }
    };

    let service = NodeServiceImpl::new(pool.clone());

    match service.list_nodes(org_id).await {
        Ok(nodes) => (StatusCode::OK, Json(nodes)).into_response(),
        Err(error) => node_error_response(error, "failed to list nodes"),
    }
}

/// List local projects associated with a node, accepting either user JWT or node API key authentication.
///
/// Verifies the node exists, then enforces access:
/// - For user auth, ensures the user is a member of the node's organization.
/// - For node API key auth, ensures the API key's organization matches the node's organization.
///
/// Returns an HTTP response with status 200 and a JSON array of the node's local projects on success.
/// Returns 404 if the node does not exist, 403 if the caller is not authorized for the node's organization,
/// or an appropriate error status for other failure cases.
///
/// # Examples
///
/// ```
/// // This handler is intended to be mounted into an Axum router and exercised in integration tests.
/// // Example usage occurs via HTTP requests against the running service where the request is
/// // authenticated either with a user JWT or an `x-api-key` header.
/// ```
#[instrument(
name = "nodes.list_projects_sync",
skip(state, auth_ctx),
fields(node_id = %node_id)
)]
pub async fn list_node_projects_sync(
    State(state): State<AppState>,
    Extension(auth_ctx): Extension<AuthContext>,
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

    // Verify access based on auth type
    match &auth_ctx {
        AuthContext::User(user_ctx) => {
            if let Err(error) = ensure_member_access(pool, node.organization_id, user_ctx.user.id).await {
                return error.into_response();
            }
        }
        AuthContext::Node(node_ctx) => {
            // For node auth, verify node belongs to same organization as API key
            if node.organization_id != node_ctx.organization_id {
                return (
                    StatusCode::FORBIDDEN,
                    Json(json!({ "error": "Node belongs to different organization" })),
                )
                    .into_response();
            }
        }
    }

    match service.list_node_local_projects(node_id).await {
        Ok(projects) => (StatusCode::OK, Json(projects)).into_response(),
        Err(error) => node_error_response(error, "failed to list node projects"),
    }
}

/// Lists the linked (swarm-project-associated) projects for a node, accepting either user JWT or node API key authentication.
///
/// The handler verifies that the caller has access to the node's organization: for user authentication it requires the user to be a member of the node's organization, and for node API key authentication it requires the API key's organization to match the node's organization. On success returns an HTTP JSON response containing the linked projects; on failure returns an appropriate JSON error response and HTTP status.
///
/// # Examples
///
/// ```
/// use axum::{Router, routing::get};
///
/// // Register the handler on a router (the handler enforces its own auth checks at runtime).
/// let app = Router::new().route("/nodes/:id/projects/linked", get(crate::routes::nodes::list_linked_node_projects_sync));
/// ```
#[instrument(
name = "nodes.list_linked_projects_sync",
skip(state, auth_ctx),
fields(node_id = %node_id)
)]
pub async fn list_linked_node_projects_sync(
    State(state): State<AppState>,
    Extension(auth_ctx): Extension<AuthContext>,
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

    // Verify access based on auth type
    match &auth_ctx {
        AuthContext::User(user_ctx) => {
            if let Err(error) = ensure_member_access(pool, node.organization_id, user_ctx.user.id).await {
                return error.into_response();
            }
        }
        AuthContext::Node(node_ctx) => {
            // For node auth, verify node belongs to same organization as API key
            if node.organization_id != node_ctx.organization_id {
                return (
                    StatusCode::FORBIDDEN,
                    Json(json!({ "error": "Node belongs to different organization" })),
                )
                    .into_response();
            }
        }
    }

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
    match service
        .list_task_attempts_by_shared_task(shared_task_id)
        .await
    {
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
    let is_complete = executions
        .iter()
        .all(|e| matches!(e.status.as_str(), "completed" | "failed" | "killed"));

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