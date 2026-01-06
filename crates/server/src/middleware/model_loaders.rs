use axum::{
    extract::{Path, Request, State},
    http::{HeaderMap, StatusCode},
    middleware::Next,
    response::Response,
};
use db::models::{
    execution_process::ExecutionProcess, label::Label, project::Project, task::Task,
    task_attempt::TaskAttempt, template::Template,
};
use deployment::Deployment;
use uuid::Uuid;

use crate::DeploymentImpl;

/// Context for an authenticated proxy request from another node.
#[derive(Debug, Clone)]
pub struct ProxyRequestContext {
    /// The ID of the node that sent the request
    pub source_node_id: String,
}

/// Extract bearer token from Authorization header.
fn extract_bearer_token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(axum::http::header::AUTHORIZATION)?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
        .or_else(|| {
            headers
                .get(axum::http::header::AUTHORIZATION)?
                .to_str()
                .ok()?
                .strip_prefix("bearer ")
        })
}

/// Context for remote project operations.
///
/// This is injected into request extensions when the project is remote
/// (`project.is_remote == true`), enabling route handlers to proxy
/// requests to the appropriate remote node.
#[derive(Debug, Clone)]
pub struct RemoteProjectContext {
    /// The UUID of the remote node that owns this project
    pub node_id: Uuid,
    /// The public URL of the remote node (e.g., "https://node.example.com")
    pub node_url: Option<String>,
    /// Current status of the remote node ("online", "offline", etc.)
    pub node_status: Option<String>,
    /// The Hive project ID used for cross-node identification
    pub remote_project_id: Uuid,
}

impl RemoteProjectContext {
    /// Check if the remote node is available for proxying.
    pub fn is_available(&self) -> bool {
        self.node_url.is_some() && self.node_status.as_deref() == Some("online")
    }

    /// Get the node URL, returning None if not configured.
    pub fn node_url(&self) -> Option<&str> {
        self.node_url.as_deref()
    }
}

/// Context for remote task attempt operations.
///
/// This is injected into request extensions when the task attempt belongs to a
/// remote project (`project.is_remote == true`), enabling route handlers to proxy
/// requests to the appropriate remote node where the attempt lives.
#[derive(Debug, Clone)]
pub struct RemoteTaskAttemptContext {
    /// The UUID of the remote node that owns this task's project
    pub node_id: Uuid,
    /// The public URL of the remote node (e.g., "https://node.example.com")
    pub node_url: Option<String>,
    /// Current status of the remote node ("online", "offline", etc.)
    pub node_status: Option<String>,
    /// The shared_task_id used for cross-node routing
    pub task_id: Uuid,
}

impl RemoteTaskAttemptContext {
    /// Check if the remote node is available for proxying.
    pub fn is_available(&self) -> bool {
        self.node_url.is_some() && self.node_status.as_deref() == Some("online")
    }

    /// Get the node URL, returning None if not configured.
    pub fn node_url(&self) -> Option<&str> {
        self.node_url.as_deref()
    }
}

pub async fn load_project_middleware(
    State(deployment): State<DeploymentImpl>,
    Path(project_id): Path<Uuid>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    load_project_impl(deployment, project_id, request, next).await
}

/// Variant of load_project_middleware for routes with wildcard path params.
/// Use this for routes like `/{id}/files/{*file_path}` where there are 2 path params.
pub async fn load_project_middleware_with_wildcard(
    State(deployment): State<DeploymentImpl>,
    Path((project_id, _file_path)): Path<(Uuid, String)>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    load_project_impl(deployment, project_id, request, next).await
}

/// Internal implementation shared by load_project_middleware variants.
async fn load_project_impl(
    deployment: DeploymentImpl,
    project_id: Uuid,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    // Load the project from the database
    let mut project = match Project::find_by_id(&deployment.db().pool, project_id).await {
        Ok(Some(project)) => project,
        Ok(None) => {
            tracing::warn!("Project {} not found", project_id);
            return Err(StatusCode::NOT_FOUND);
        }
        Err(e) => {
            tracing::error!("Failed to fetch project {}: {}", project_id, e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    // For local projects linked to Hive, populate source_node_name for symmetric display.
    // This ensures task cards show the node indicator on all nodes, not just remote ones.
    if !project.is_remote
        && project.remote_project_id.is_some()
        && project.source_node_name.is_none()
    {
        project.source_node_name = Some(gethostname::gethostname().to_string_lossy().to_string());
    }

    let mut request = request;

    // If project is remote, inject RemoteProjectContext for proxy routing
    if project.is_remote {
        if let (Some(node_id), Some(remote_project_id)) =
            (project.source_node_id, project.remote_project_id)
        {
            let remote_ctx = RemoteProjectContext {
                node_id,
                node_url: project.source_node_public_url.clone(),
                node_status: project.source_node_status.clone(),
                remote_project_id,
            };
            tracing::debug!(
                project_id = %project_id,
                node_id = %node_id,
                remote_project_id = %remote_project_id,
                node_status = ?remote_ctx.node_status,
                "Loaded remote project context"
            );
            request.extensions_mut().insert(remote_ctx);
        } else {
            tracing::warn!(
                project_id = %project_id,
                "Remote project missing source_node_id or remote_project_id"
            );
        }
    }

    // Insert the project as an extension
    request.extensions_mut().insert(project);

    // Continue with the next middleware/handler
    Ok(next.run(request).await)
}

/// Middleware to load a project by its remote (Hive) project ID.
///
/// Used for the `/projects/by-remote-id/{remote_project_id}` routes
/// that receive proxied requests from other nodes.
///
/// This middleware also validates the proxy token if one is provided via
/// the Authorization header. If no token is provided and the connection token
/// validator is enabled, the request is rejected.
pub async fn load_project_by_remote_id_middleware(
    State(deployment): State<DeploymentImpl>,
    Path(remote_project_id): Path<Uuid>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let mut request = request;

    // Validate proxy token if connection token validation is enabled
    let validator = deployment.connection_token_validator();
    if validator.is_enabled() {
        let token = extract_bearer_token(request.headers()).ok_or_else(|| {
            tracing::warn!(
                remote_project_id = %remote_project_id,
                "Missing Authorization header for by-remote-id route"
            );
            StatusCode::UNAUTHORIZED
        })?;

        match validator.validate_proxy_token(token) {
            Ok(proxy_token) => {
                tracing::debug!(
                    source_node_id = %proxy_token.source_node_id,
                    target_node_id = %proxy_token.target_node_id,
                    remote_project_id = %remote_project_id,
                    "Validated proxy token"
                );
                request.extensions_mut().insert(ProxyRequestContext {
                    source_node_id: proxy_token.source_node_id,
                });
            }
            Err(e) => {
                tracing::warn!(
                    remote_project_id = %remote_project_id,
                    error = ?e,
                    "Invalid proxy token"
                );
                return Err(StatusCode::UNAUTHORIZED);
            }
        }
    }

    // Load the project by its remote_project_id
    let project =
        match Project::find_by_remote_project_id(&deployment.db().pool, remote_project_id).await {
            Ok(Some(project)) => project,
            Ok(None) => {
                tracing::warn!(
                    remote_project_id = %remote_project_id,
                    "Project not found by remote_project_id"
                );
                return Err(StatusCode::NOT_FOUND);
            }
            Err(e) => {
                tracing::error!(
                    remote_project_id = %remote_project_id,
                    error = %e,
                    "Failed to fetch project by remote_project_id"
                );
                return Err(StatusCode::INTERNAL_SERVER_ERROR);
            }
        };

    tracing::debug!(
        local_project_id = %project.id,
        remote_project_id = %remote_project_id,
        "Loaded project by remote_project_id"
    );

    // Insert the project as an extension
    request.extensions_mut().insert(project);

    // Continue with the next middleware/handler
    Ok(next.run(request).await)
}

pub async fn load_task_middleware(
    State(deployment): State<DeploymentImpl>,
    Path(task_id): Path<Uuid>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    // Load the task and validate it belongs to the project
    let task = match Task::find_by_id(&deployment.db().pool, task_id).await {
        Ok(Some(task)) => task,
        Ok(None) => {
            tracing::warn!("Task {} not found", task_id);
            return Err(StatusCode::NOT_FOUND);
        }
        Err(e) => {
            tracing::error!("Failed to fetch task {}: {}", task_id, e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    // Insert both models as extensions
    let mut request = request;
    request.extensions_mut().insert(task);

    // Continue with the next middleware/handler
    Ok(next.run(request).await)
}

pub async fn load_task_attempt_middleware(
    State(deployment): State<DeploymentImpl>,
    Path(task_attempt_id): Path<Uuid>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    load_task_attempt_impl(deployment, task_attempt_id, request, next).await
}

/// Variant of load_task_attempt_middleware for routes with wildcard path params.
/// Use this for routes like `/{id}/files/{*file_path}` where there are 2 path params.
pub async fn load_task_attempt_middleware_with_wildcard(
    State(deployment): State<DeploymentImpl>,
    Path((task_attempt_id, _file_path)): Path<(Uuid, String)>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    load_task_attempt_impl(deployment, task_attempt_id, request, next).await
}

/// Variant of load_task_attempt_middleware for routes with two UUID path params.
/// Use this for routes like `/{id}/.../{uuid}` where both are UUIDs.
pub async fn load_task_attempt_middleware_with_uuid_suffix(
    State(deployment): State<DeploymentImpl>,
    Path((task_attempt_id, _suffix_id)): Path<(Uuid, Uuid)>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    load_task_attempt_impl(deployment, task_attempt_id, request, next).await
}

/// Internal implementation shared by load_task_attempt_middleware variants.
async fn load_task_attempt_impl(
    deployment: DeploymentImpl,
    task_attempt_id: Uuid,
    mut request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    // Load the TaskAttempt from the database
    let attempt = match TaskAttempt::find_by_id(&deployment.db().pool, task_attempt_id).await {
        Ok(Some(a)) => a,
        Ok(None) => {
            tracing::warn!("TaskAttempt {} not found", task_attempt_id);
            return Err(StatusCode::NOT_FOUND);
        }
        Err(e) => {
            tracing::error!("Failed to fetch TaskAttempt {}: {}", task_attempt_id, e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    // Load the parent Task to check if it belongs to a remote project
    let task = match Task::find_by_id(&deployment.db().pool, attempt.task_id).await {
        Ok(Some(t)) => t,
        Ok(None) => {
            tracing::warn!(
                "Task {} not found for TaskAttempt {}",
                attempt.task_id,
                task_attempt_id
            );
            return Err(StatusCode::NOT_FOUND);
        }
        Err(e) => {
            tracing::error!(
                "Failed to fetch Task {} for TaskAttempt {}: {}",
                attempt.task_id,
                task_attempt_id,
                e
            );
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    // Load the Project to check if it's remote
    let project = match Project::find_by_id(&deployment.db().pool, task.project_id).await {
        Ok(Some(p)) => p,
        Ok(None) => {
            tracing::warn!("Project {} not found for Task {}", task.project_id, task.id);
            return Err(StatusCode::NOT_FOUND);
        }
        Err(e) => {
            tracing::error!(
                "Failed to fetch Project {} for Task {}: {}",
                task.project_id,
                task.id,
                e
            );
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    // If project is remote, inject RemoteTaskAttemptContext for proxy routing
    if project.is_remote {
        if let (Some(node_id), Some(shared_task_id)) = (project.source_node_id, task.shared_task_id)
        {
            let remote_ctx = RemoteTaskAttemptContext {
                node_id,
                node_url: project.source_node_public_url.clone(),
                node_status: project.source_node_status.clone(),
                task_id: shared_task_id,
            };
            tracing::debug!(
                task_attempt_id = %task_attempt_id,
                task_id = %task.id,
                shared_task_id = %shared_task_id,
                node_id = %node_id,
                node_status = ?remote_ctx.node_status,
                "Loaded remote task attempt context"
            );
            request.extensions_mut().insert(remote_ctx);
        } else {
            tracing::warn!(
                task_attempt_id = %task_attempt_id,
                task_id = %task.id,
                project_id = %project.id,
                "Remote project missing source_node_id or task missing shared_task_id"
            );
        }
    }

    // Insert the attempt into extensions
    request.extensions_mut().insert(attempt);

    // Continue on
    Ok(next.run(request).await)
}

/// Middleware to load a task attempt by the task's shared_task_id.
///
/// Used for the `/task-attempts/by-task-id/{task_id}` routes that receive
/// proxied requests from other nodes. This finds the task by shared_task_id,
/// then loads its most recent attempt.
///
/// This middleware also validates the proxy token if one is provided via
/// the Authorization header. If no token is provided and the connection token
/// validator is enabled, the request is rejected.
pub async fn load_task_attempt_by_task_id_middleware(
    State(deployment): State<DeploymentImpl>,
    Path(shared_task_id): Path<Uuid>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    load_task_attempt_by_task_id_impl(deployment, shared_task_id, request, next).await
}

/// Variant for routes with wildcard path params like `/{task_id}/files/{*file_path}`.
pub async fn load_task_attempt_by_task_id_middleware_with_wildcard(
    State(deployment): State<DeploymentImpl>,
    Path((shared_task_id, _file_path)): Path<(Uuid, String)>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    load_task_attempt_by_task_id_impl(deployment, shared_task_id, request, next).await
}

/// Internal implementation for loading task attempt by shared_task_id.
async fn load_task_attempt_by_task_id_impl(
    deployment: DeploymentImpl,
    shared_task_id: Uuid,
    mut request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    // Validate proxy token if connection token validation is enabled
    let validator = deployment.connection_token_validator();
    if validator.is_enabled() {
        let token = extract_bearer_token(request.headers()).ok_or_else(|| {
            tracing::warn!(
                shared_task_id = %shared_task_id,
                "Missing Authorization header for by-task-id route"
            );
            StatusCode::UNAUTHORIZED
        })?;

        match validator.validate_proxy_token(token) {
            Ok(proxy_token) => {
                tracing::debug!(
                    source_node_id = %proxy_token.source_node_id,
                    target_node_id = %proxy_token.target_node_id,
                    shared_task_id = %shared_task_id,
                    "Validated proxy token for by-task-id route"
                );
                request.extensions_mut().insert(ProxyRequestContext {
                    source_node_id: proxy_token.source_node_id,
                });
            }
            Err(e) => {
                tracing::warn!(
                    shared_task_id = %shared_task_id,
                    error = ?e,
                    "Invalid proxy token for by-task-id route"
                );
                return Err(StatusCode::UNAUTHORIZED);
            }
        }
    }

    // Find the task by shared_task_id
    let task = match Task::find_by_shared_task_id(&deployment.db().pool, shared_task_id).await {
        Ok(Some(task)) => task,
        Ok(None) => {
            tracing::warn!(
                shared_task_id = %shared_task_id,
                "Task not found by shared_task_id"
            );
            return Err(StatusCode::NOT_FOUND);
        }
        Err(e) => {
            tracing::error!(
                shared_task_id = %shared_task_id,
                error = %e,
                "Failed to fetch task by shared_task_id"
            );
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    // Find the most recent attempt for this task
    let attempts = match TaskAttempt::fetch_all(&deployment.db().pool, Some(task.id)).await {
        Ok(attempts) => attempts,
        Err(e) => {
            tracing::error!(
                task_id = %task.id,
                shared_task_id = %shared_task_id,
                error = %e,
                "Failed to fetch task attempts"
            );
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    let attempt = match attempts.into_iter().next() {
        Some(attempt) => attempt,
        None => {
            tracing::warn!(
                task_id = %task.id,
                shared_task_id = %shared_task_id,
                "No task attempts found for task"
            );
            return Err(StatusCode::NOT_FOUND);
        }
    };

    tracing::debug!(
        attempt_id = %attempt.id,
        task_id = %task.id,
        shared_task_id = %shared_task_id,
        "Loaded task attempt by shared_task_id"
    );

    // Insert the attempt into extensions
    request.extensions_mut().insert(attempt);

    // Continue to the next middleware/handler
    Ok(next.run(request).await)
}

/// Middleware that loads a Task by shared_task_id for routes that don't need an existing attempt.
/// Used for creating new task attempts via cross-node proxying.
pub async fn load_task_by_task_id_middleware(
    State(deployment): State<DeploymentImpl>,
    Path(shared_task_id): Path<Uuid>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    load_task_by_task_id_impl(deployment, shared_task_id, request, next).await
}

/// Internal implementation for loading task by shared_task_id (without attempt lookup).
async fn load_task_by_task_id_impl(
    deployment: DeploymentImpl,
    shared_task_id: Uuid,
    mut request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    // Validate proxy token if connection token validation is enabled
    let validator = deployment.connection_token_validator();
    if validator.is_enabled() {
        let token = extract_bearer_token(request.headers()).ok_or_else(|| {
            tracing::warn!(
                shared_task_id = %shared_task_id,
                "Missing Authorization header for by-task-id route"
            );
            StatusCode::UNAUTHORIZED
        })?;

        match validator.validate_proxy_token(token) {
            Ok(proxy_token) => {
                tracing::debug!(
                    source_node_id = %proxy_token.source_node_id,
                    target_node_id = %proxy_token.target_node_id,
                    "Validated proxy token for by-task-id route"
                );
            }
            Err(e) => {
                tracing::warn!(
                    shared_task_id = %shared_task_id,
                    error = ?e,
                    "Invalid proxy token for by-task-id route"
                );
                return Err(StatusCode::UNAUTHORIZED);
            }
        }
    }

    // Find the task by shared_task_id
    let task = match Task::find_by_shared_task_id(&deployment.db().pool, shared_task_id).await {
        Ok(Some(task)) => task,
        Ok(None) => {
            tracing::warn!(
                shared_task_id = %shared_task_id,
                "Task not found by shared_task_id"
            );
            return Err(StatusCode::NOT_FOUND);
        }
        Err(e) => {
            tracing::error!(
                shared_task_id = %shared_task_id,
                error = %e,
                "Failed to fetch task by shared_task_id"
            );
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    // Inject the task into the request
    request.extensions_mut().insert(task);

    // Continue to the next middleware/handler
    Ok(next.run(request).await)
}

pub async fn load_execution_process_middleware(
    State(deployment): State<DeploymentImpl>,
    Path(process_id): Path<Uuid>,
    mut request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    // Load the execution process from the database
    let execution_process =
        match ExecutionProcess::find_by_id(&deployment.db().pool, process_id).await {
            Ok(Some(process)) => process,
            Ok(None) => {
                tracing::warn!("ExecutionProcess {} not found", process_id);
                return Err(StatusCode::NOT_FOUND);
            }
            Err(e) => {
                tracing::error!("Failed to fetch execution process {}: {}", process_id, e);
                return Err(StatusCode::INTERNAL_SERVER_ERROR);
            }
        };

    // Inject the execution process into the request
    request.extensions_mut().insert(execution_process);

    // Continue to the next middleware/handler
    Ok(next.run(request).await)
}

// Middleware that loads and injects Template based on the template_id path parameter
pub async fn load_template_middleware(
    State(deployment): State<DeploymentImpl>,
    Path(template_id): Path<Uuid>,
    request: axum::extract::Request,
    next: Next,
) -> Result<Response, StatusCode> {
    // Load the template from the database
    let template = match Template::find_by_id(&deployment.db().pool, template_id).await {
        Ok(Some(template)) => template,
        Ok(None) => {
            tracing::warn!("Template {} not found", template_id);
            return Err(StatusCode::NOT_FOUND);
        }
        Err(e) => {
            tracing::error!("Failed to fetch template {}: {}", template_id, e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    // Insert the template as an extension
    let mut request = request;
    request.extensions_mut().insert(template);

    // Continue with the next middleware/handler
    Ok(next.run(request).await)
}

// Middleware that loads and injects Label based on the label_id path parameter
pub async fn load_label_middleware(
    State(deployment): State<DeploymentImpl>,
    Path(label_id): Path<Uuid>,
    request: axum::extract::Request,
    next: Next,
) -> Result<Response, StatusCode> {
    // Load the label from the database
    let label = match Label::find_by_id(&deployment.db().pool, label_id).await {
        Ok(Some(label)) => label,
        Ok(None) => {
            tracing::warn!("Label {} not found", label_id);
            return Err(StatusCode::NOT_FOUND);
        }
        Err(e) => {
            tracing::error!("Failed to fetch label {}: {}", label_id, e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    // Insert the label as an extension
    let mut request = request;
    request.extensions_mut().insert(label);

    // Continue with the next middleware/handler
    Ok(next.run(request).await)
}
