//! Core CRUD handlers: create, get, list, status, children, sessions, dev server.

use axum::{
    Extension, Json,
    extract::{Query, State},
    response::Json as ResponseJson,
};
use db::models::{
    execution_process::{ExecutionProcess, ExecutionProcessRunReason, ExecutionProcessStatus},
    execution_process_logs::ExecutionProcessLogs,
    executor_session::ExecutorSession,
    project::Project,
    task::{Task, TaskRelationships},
    task_attempt::{CreateTaskAttempt, TaskAttempt, TaskAttemptError},
};
use deployment::Deployment;
use executors::{
    actions::{
        ExecutorAction, ExecutorActionType,
        script::{ScriptContext, ScriptRequest, ScriptRequestLanguage},
    },
    executors::{CodingAgent, ExecutorError},
    profile::ExecutorConfigs,
};
use services::services::container::ContainerService;
use sqlx::Error as SqlxError;
use utils::response::ApiResponse;
use uuid::Uuid;

use crate::routes::task_attempts::codex_setup;
use crate::routes::task_attempts::cursor_setup;
use crate::routes::task_attempts::types::{
    CommitCompareResult, CommitInfo, CreateTaskAttemptBody, CreateTaskAttemptByTaskIdBody,
    FixSessionsResponse, OpenEditorRequest, OpenEditorResponse, RunAgentSetupRequest,
    RunAgentSetupResponse, TaskAttemptQuery,
};
use crate::routes::task_attempts::util::ensure_worktree_path;
use crate::{
    DeploymentImpl,
    error::ApiError,
    middleware::{RemoteAttemptNeeded, RemoteTaskAttemptContext},
    proxy::check_remote_task_attempt_proxy,
};

// ============================================================================
// List and Get
// ============================================================================

pub async fn get_task_attempts(
    State(deployment): State<DeploymentImpl>,
    Query(query): Query<TaskAttemptQuery>,
) -> Result<ResponseJson<ApiResponse<Vec<TaskAttempt>>>, ApiError> {
    let pool = &deployment.db().pool;

    // If task_id is provided, check if it's a swarm task and query Hive
    if let Some(task_id) = query.task_id
        && let Some(task) = Task::find_by_id(pool, task_id).await?
        && let Some(shared_task_id) = task.shared_task_id
        && let Ok(client) = deployment.remote_client()
    {
        // Try to get attempts from Hive
        match client
            .list_task_attempts_by_shared_task(shared_task_id)
            .await
        {
            Ok(hive_attempts) => {
                // Convert NodeTaskAttempt to TaskAttempt
                let attempts: Vec<TaskAttempt> = hive_attempts
                    .into_iter()
                    .map(|nta| TaskAttempt {
                        id: nta.id,
                        task_id, // Map back to local task_id
                        container_ref: nta.container_ref,
                        branch: nta.branch,
                        target_branch: nta.target_branch,
                        executor: nta.executor,
                        worktree_deleted: nta.worktree_deleted,
                        setup_completed_at: nta.setup_completed_at,
                        created_at: nta.created_at,
                        updated_at: nta.updated_at,
                        hive_synced_at: Some(nta.updated_at), // Came from Hive, so it's synced
                        hive_assignment_id: nta.assignment_id,
                    })
                    .collect();
                return Ok(ResponseJson(ApiResponse::success(attempts)));
            }
            Err(e) => {
                tracing::warn!(
                    shared_task_id = %shared_task_id,
                    error = %e,
                    "Failed to fetch task attempts from Hive, falling back to local"
                );
                // Fall through to local query
            }
        }
    }

    // Fall back to local attempts
    let attempts = TaskAttempt::fetch_all(pool, query.task_id).await?;
    Ok(ResponseJson(ApiResponse::success(attempts)))
}

pub async fn get_task_attempt(
    local_attempt: Option<Extension<TaskAttempt>>,
    remote_needed: Option<Extension<RemoteAttemptNeeded>>,
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<TaskAttempt>>, ApiError> {
    // If we have a local attempt, return it
    if let Some(Extension(attempt)) = local_attempt {
        return Ok(ResponseJson(ApiResponse::success(attempt)));
    }

    // If attempt not found locally, try Hive fallback
    if let Some(Extension(remote)) = remote_needed
        && let Ok(client) = deployment.remote_client()
    {
        match client.get_node_task_attempt(remote.attempt_id).await {
            Ok(Some(hive_response)) => {
                // Find local task by shared_task_id to map back to local task_id
                let pool = &deployment.db().pool;
                let task = Task::find_by_shared_task_id(pool, hive_response.attempt.shared_task_id)
                    .await?;

                if let Some(task) = task {
                    // Convert NodeTaskAttempt to local TaskAttempt format
                    let attempt = TaskAttempt {
                        id: hive_response.attempt.id,
                        task_id: task.id, // Map to local task_id
                        container_ref: hive_response.attempt.container_ref,
                        branch: hive_response.attempt.branch,
                        target_branch: hive_response.attempt.target_branch,
                        executor: hive_response.attempt.executor,
                        worktree_deleted: hive_response.attempt.worktree_deleted,
                        setup_completed_at: hive_response.attempt.setup_completed_at,
                        created_at: hive_response.attempt.created_at,
                        updated_at: hive_response.attempt.updated_at,
                        hive_synced_at: Some(hive_response.attempt.updated_at),
                        hive_assignment_id: hive_response.attempt.assignment_id,
                    };
                    return Ok(ResponseJson(ApiResponse::success(attempt)));
                }
                // Task not found locally - can't map the attempt
                tracing::warn!(
                    attempt_id = %remote.attempt_id,
                    shared_task_id = %hive_response.attempt.shared_task_id,
                    "Attempt found on Hive but shared task not linked locally"
                );
            }
            Ok(None) => {
                tracing::debug!(
                    attempt_id = %remote.attempt_id,
                    "Attempt not found on Hive"
                );
            }
            Err(e) => {
                tracing::warn!(
                    attempt_id = %remote.attempt_id,
                    error = %e,
                    "Failed to fetch attempt from Hive"
                );
            }
        }
    }

    Err(ApiError::NotFound("Task attempt not found".to_string()))
}

// ============================================================================
// Create
// ============================================================================

#[axum::debug_handler]
pub async fn create_task_attempt(
    State(deployment): State<DeploymentImpl>,
    Json(payload): Json<CreateTaskAttemptBody>,
) -> Result<ResponseJson<ApiResponse<TaskAttempt>>, ApiError> {
    // If target_node_id is specified, proxy to the target node
    if let Some(target_node_id) = payload.target_node_id {
        let task = Task::find_by_id(&deployment.db().pool, payload.task_id)
            .await?
            .ok_or(SqlxError::RowNotFound)?;

        // Need shared_task_id to route to remote node
        let shared_task_id = task.shared_task_id.ok_or_else(|| {
            ApiError::BadRequest("Task is not shared (no shared_task_id)".to_string())
        })?;

        // Query the hive to get node info (URL and status)
        let client = deployment.remote_client()?;
        let project = Project::find_by_id(&deployment.db().pool, task.project_id)
            .await?
            .ok_or(SqlxError::RowNotFound)?;

        let remote_project_id = project
            .remote_project_id
            .ok_or_else(|| ApiError::BadRequest("Project is not linked to hive".to_string()))?;

        let nodes_response = client.list_project_nodes(remote_project_id).await?;

        // Find the target node in the response
        let target_node = nodes_response
            .nodes
            .iter()
            .find(|n| n.node_id == target_node_id)
            .ok_or_else(|| {
                ApiError::BadRequest(format!(
                    "Target node {} does not have this project linked",
                    target_node_id
                ))
            })?;

        // Check if node is online
        if target_node.node_status != remote::nodes::NodeStatus::Online {
            return Err(ApiError::BadGateway(format!(
                "Target node '{}' is not online (status: {:?})",
                target_node.node_name, target_node.node_status
            )));
        }

        let node_url = target_node.node_public_url.clone().ok_or_else(|| {
            ApiError::BadGateway(format!(
                "Target node '{}' does not have a public URL configured",
                target_node.node_name
            ))
        })?;

        // Build the proxy request
        let proxy_body = CreateTaskAttemptByTaskIdBody {
            executor_profile_id: payload.executor_profile_id,
            base_branch: payload.base_branch,
            use_parent_worktree: payload.use_parent_worktree,
        };

        let path = format!("/task-attempts/by-task-id/{}/create", shared_task_id);
        tracing::info!(
            target_node_id = %target_node_id,
            target_node_name = %target_node.node_name,
            shared_task_id = %shared_task_id,
            "Proxying create_task_attempt to remote node"
        );

        let response: ApiResponse<TaskAttempt> = deployment
            .node_proxy_client()
            .proxy_post(&node_url, &path, &proxy_body, target_node_id)
            .await?;

        // Set the executing node in the Hive so it can be displayed in the UI
        if response.is_success()
            && let Ok(client) = deployment.remote_client()
            && let Err(e) = client
                .set_executing_node(shared_task_id, Some(target_node_id))
                .await
        {
            tracing::warn!(
                error = ?e,
                shared_task_id = %shared_task_id,
                target_node_id = %target_node_id,
                "Failed to set executing node in Hive (non-blocking)"
            );
        }

        return Ok(ResponseJson(response));
    }

    // Local execution path
    let executor_profile_id = payload.get_executor_profile_id();
    let pool = &deployment.db().pool;
    let task = Task::find_by_id(pool, payload.task_id)
        .await?
        .ok_or(SqlxError::RowNotFound)?;

    // Auto-unarchive the task when a new attempt is created
    if Task::unarchive_if_archived(pool, task.id).await? {
        tracing::info!(
            task_id = %task.id,
            "Auto-unarchived task due to new attempt creation"
        );

        // Sync unarchive to Hive if task is shared
        if task.shared_task_id.is_some()
            && let Ok(publisher) = deployment.share_publisher()
            && let Some(updated_task) = Task::find_by_id(pool, task.id).await?
        {
            let publisher = publisher.clone();
            tokio::spawn(async move {
                if let Err(e) = publisher.update_shared_task(&updated_task).await {
                    tracing::warn!(?e, "failed to sync task unarchive to Hive");
                }
            });
        }
    }

    let attempt_id = Uuid::new_v4();

    // Determine branch name and parent worktree info based on use_parent_worktree flag
    let (git_branch_name, parent_container_ref) = if payload.use_parent_worktree.unwrap_or(false) {
        // Validate task has parent
        let parent_task_id = task.parent_task_id.ok_or_else(|| {
            ApiError::BadRequest("Cannot use parent worktree: task has no parent_task_id".into())
        })?;

        // Get parent task's latest attempt
        let parent_attempts = TaskAttempt::fetch_all(pool, Some(parent_task_id)).await?;
        let parent_attempt = parent_attempts.first().ok_or_else(|| {
            ApiError::BadRequest("Cannot use parent worktree: parent task has no attempts".into())
        })?;

        // Validate parent has a worktree
        let container_ref = parent_attempt.container_ref.clone().ok_or_else(|| {
            ApiError::BadRequest(
                "Cannot use parent worktree: parent attempt has no worktree".into(),
            )
        })?;

        // Validate parent worktree not deleted
        if parent_attempt.worktree_deleted {
            return Err(ApiError::BadRequest(
                "Cannot use parent worktree: parent worktree was deleted".into(),
            ));
        }

        (parent_attempt.branch.clone(), Some(container_ref))
    } else {
        let branch = deployment
            .container()
            .git_branch_from_task_attempt(&attempt_id, &task.title)
            .await;
        (branch, None)
    };

    let task_attempt = TaskAttempt::create(
        pool,
        &CreateTaskAttempt {
            executor: executor_profile_id.executor,
            base_branch: payload.base_branch.clone(),
            branch: git_branch_name.clone(),
        },
        attempt_id,
        payload.task_id,
    )
    .await?;

    // If using parent worktree, update container_ref before calling start_attempt
    let skip_worktree_creation = if let Some(ref container_ref) = parent_container_ref {
        TaskAttempt::update_container_ref(pool, task_attempt.id, container_ref).await?;
        tracing::info!(
            task_id = %task.id,
            attempt_id = %task_attempt.id,
            container_ref = %container_ref,
            "Using parent worktree for attempt"
        );
        true
    } else {
        false
    };

    // Refetch to get the updated container_ref before starting attempt
    let task_attempt = TaskAttempt::find_by_id(pool, task_attempt.id)
        .await?
        .ok_or(SqlxError::RowNotFound)?;

    // Start the attempt (creates worktree if needed, then starts execution)
    if let Err(err) = deployment
        .container()
        .start_attempt(
            &task_attempt,
            executor_profile_id.clone(),
            skip_worktree_creation,
        )
        .await
    {
        tracing::error!("Failed to start task attempt: {}", err);
    }
    tracing::info!("Created attempt for task {}", task.id);

    // Refetch to get the final state
    let task_attempt = TaskAttempt::find_by_id(pool, task_attempt.id)
        .await?
        .ok_or(SqlxError::RowNotFound)?;

    Ok(ResponseJson(ApiResponse::success(task_attempt)))
}

/// Create a task attempt via by-task-id route (used for cross-node proxying).
/// The task is loaded by shared_task_id from the URL path parameter.
#[axum::debug_handler]
pub async fn create_task_attempt_by_task_id(
    Extension(task): Extension<Task>,
    State(deployment): State<DeploymentImpl>,
    Json(payload): Json<CreateTaskAttemptByTaskIdBody>,
) -> Result<ResponseJson<ApiResponse<TaskAttempt>>, ApiError> {
    let executor_profile_id = payload.executor_profile_id.clone();
    let pool = &deployment.db().pool;

    let attempt_id = Uuid::new_v4();

    // Determine branch name and parent worktree info based on use_parent_worktree flag
    let (git_branch_name, parent_container_ref) = if payload.use_parent_worktree.unwrap_or(false) {
        // Validate task has parent
        let parent_task_id = task.parent_task_id.ok_or_else(|| {
            ApiError::BadRequest("Cannot use parent worktree: task has no parent_task_id".into())
        })?;

        // Get parent task's latest attempt
        let parent_attempts = TaskAttempt::fetch_all(pool, Some(parent_task_id)).await?;
        let parent_attempt = parent_attempts.first().ok_or_else(|| {
            ApiError::BadRequest("Cannot use parent worktree: parent task has no attempts".into())
        })?;

        // Validate parent has a worktree
        let container_ref = parent_attempt.container_ref.clone().ok_or_else(|| {
            ApiError::BadRequest(
                "Cannot use parent worktree: parent attempt has no worktree".into(),
            )
        })?;

        // Validate parent worktree not deleted
        if parent_attempt.worktree_deleted {
            return Err(ApiError::BadRequest(
                "Cannot use parent worktree: parent worktree was deleted".into(),
            ));
        }

        (parent_attempt.branch.clone(), Some(container_ref))
    } else {
        let branch = deployment
            .container()
            .git_branch_from_task_attempt(&attempt_id, &task.title)
            .await;
        (branch, None)
    };

    let task_attempt = TaskAttempt::create(
        pool,
        &CreateTaskAttempt {
            executor: executor_profile_id.executor,
            base_branch: payload.base_branch.clone(),
            branch: git_branch_name.clone(),
        },
        attempt_id,
        task.id,
    )
    .await?;

    // If using parent worktree, update container_ref before calling start_attempt
    let skip_worktree_creation = if let Some(ref container_ref) = parent_container_ref {
        TaskAttempt::update_container_ref(pool, task_attempt.id, container_ref).await?;
        tracing::info!(
            task_id = %task.id,
            shared_task_id = ?task.shared_task_id,
            attempt_id = %task_attempt.id,
            container_ref = %container_ref,
            "Using parent worktree for attempt via by-task-id route"
        );
        true
    } else {
        false
    };

    // Refetch to get the updated container_ref before starting attempt
    let task_attempt = TaskAttempt::find_by_id(pool, task_attempt.id)
        .await?
        .ok_or(SqlxError::RowNotFound)?;

    // Start the attempt (creates worktree if needed, then starts execution)
    if let Err(err) = deployment
        .container()
        .start_attempt(
            &task_attempt,
            executor_profile_id.clone(),
            skip_worktree_creation,
        )
        .await
    {
        tracing::error!("Failed to start task attempt: {}", err);
    }
    tracing::info!(
        task_id = %task.id,
        shared_task_id = ?task.shared_task_id,
        attempt_id = %task_attempt.id,
        "Created attempt via by-task-id route"
    );

    // Refetch to get the final state
    let task_attempt = TaskAttempt::find_by_id(pool, task_attempt.id)
        .await?
        .ok_or(SqlxError::RowNotFound)?;

    Ok(ResponseJson(ApiResponse::success(task_attempt)))
}

// ============================================================================
// Agent Setup
// ============================================================================

#[axum::debug_handler]
pub async fn run_agent_setup(
    Extension(task_attempt): Extension<TaskAttempt>,
    State(deployment): State<DeploymentImpl>,
    Json(payload): Json<RunAgentSetupRequest>,
) -> Result<ResponseJson<ApiResponse<RunAgentSetupResponse>>, ApiError> {
    let executor_profile_id = payload.executor_profile_id;
    let config = ExecutorConfigs::get_cached();
    let coding_agent = config.get_coding_agent_or_default(&executor_profile_id);
    match coding_agent {
        CodingAgent::CursorAgent(_) => {
            cursor_setup::run_cursor_setup(&deployment, &task_attempt).await?;
        }
        CodingAgent::Codex(codex) => {
            codex_setup::run_codex_setup(&deployment, &task_attempt, &codex).await?;
        }
        _ => return Err(ApiError::Executor(ExecutorError::SetupHelperNotSupported)),
    }

    Ok(ResponseJson(ApiResponse::success(RunAgentSetupResponse {})))
}

// ============================================================================
// Commit Info
// ============================================================================

pub async fn get_commit_info(
    Extension(task_attempt): Extension<TaskAttempt>,
    State(deployment): State<DeploymentImpl>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<ResponseJson<ApiResponse<CommitInfo>>, ApiError> {
    let Some(sha) = params.get("sha").cloned() else {
        return Err(ApiError::TaskAttempt(TaskAttemptError::ValidationError(
            "Missing sha param".to_string(),
        )));
    };
    let wt_buf = ensure_worktree_path(&deployment, &task_attempt).await?;
    let wt = wt_buf.as_path();
    let subject = deployment.git().get_commit_subject(wt, &sha)?;
    Ok(ResponseJson(ApiResponse::success(CommitInfo {
        sha,
        subject,
    })))
}

pub async fn compare_commit_to_head(
    Extension(task_attempt): Extension<TaskAttempt>,
    State(deployment): State<DeploymentImpl>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<ResponseJson<ApiResponse<CommitCompareResult>>, ApiError> {
    let Some(target_oid) = params.get("sha").cloned() else {
        return Err(ApiError::TaskAttempt(TaskAttemptError::ValidationError(
            "Missing sha param".to_string(),
        )));
    };
    let wt_buf = ensure_worktree_path(&deployment, &task_attempt).await?;
    let wt = wt_buf.as_path();
    let head_info = deployment.git().get_head_info(wt)?;
    let (ahead_from_head, behind_from_head) =
        deployment
            .git()
            .ahead_behind_commits_by_oid(wt, &head_info.oid, &target_oid)?;
    let is_linear = behind_from_head == 0;
    Ok(ResponseJson(ApiResponse::success(CommitCompareResult {
        head_oid: head_info.oid,
        target_oid,
        ahead_from_head,
        behind_from_head,
        is_linear,
    })))
}

// ============================================================================
// Editor
// ============================================================================

pub async fn open_task_attempt_in_editor(
    Extension(task_attempt): Extension<TaskAttempt>,
    State(deployment): State<DeploymentImpl>,
    Json(payload): Json<OpenEditorRequest>,
) -> Result<ResponseJson<ApiResponse<OpenEditorResponse>>, ApiError> {
    // Get the task attempt to access the worktree path
    let base_path_buf = ensure_worktree_path(&deployment, &task_attempt).await?;
    let base_path = base_path_buf.as_path();

    // If a specific file path is provided, use it; otherwise use the base path
    let path = if let Some(file_path) = payload.file_path.as_ref() {
        base_path.join(file_path)
    } else {
        base_path.to_path_buf()
    };

    let editor_config = {
        let config = deployment.config().read().await;
        let editor_type_str = payload.editor_type.as_deref();
        config.editor.with_override(editor_type_str)
    };

    match editor_config.open_file(path.as_path()).await {
        Ok(url) => {
            tracing::info!(
                "Opened editor for task attempt {} at path: {}{}",
                task_attempt.id,
                path.display(),
                if url.is_some() { " (remote mode)" } else { "" }
            );

            Ok(ResponseJson(ApiResponse::success(OpenEditorResponse {
                url,
            })))
        }
        Err(e) => {
            tracing::error!(
                "Failed to open editor for attempt {}: {:?}",
                task_attempt.id,
                e
            );
            Err(ApiError::EditorOpen(e))
        }
    }
}

// ============================================================================
// Dev Server
// ============================================================================

#[axum::debug_handler]
pub async fn start_dev_server(
    Extension(task_attempt): Extension<TaskAttempt>,
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<()>>, ApiError> {
    let pool = &deployment.db().pool;

    // Get parent task
    let task = task_attempt
        .parent_task(&deployment.db().pool)
        .await?
        .ok_or(SqlxError::RowNotFound)?;

    // Get parent project
    let project = task
        .parent_project(&deployment.db().pool)
        .await?
        .ok_or(SqlxError::RowNotFound)?;

    // Stop any existing dev servers for this project
    let existing_dev_servers =
        match ExecutionProcess::find_running_dev_servers_by_project(pool, project.id).await {
            Ok(servers) => servers,
            Err(e) => {
                tracing::error!(
                    "Failed to find running dev servers for project {}: {}",
                    project.id,
                    e
                );
                return Err(ApiError::TaskAttempt(TaskAttemptError::ValidationError(
                    e.to_string(),
                )));
            }
        };

    for dev_server in existing_dev_servers {
        tracing::info!(
            "Stopping existing dev server {} for project {}",
            dev_server.id,
            project.id
        );

        if let Err(e) = deployment
            .container()
            .stop_execution(&dev_server, ExecutionProcessStatus::Killed)
            .await
        {
            tracing::error!("Failed to stop dev server {}: {}", dev_server.id, e);
        }
    }

    if let Some(dev_server) = project.dev_script {
        // TODO: Derive script language from system config
        let executor_action = ExecutorAction::new(
            ExecutorActionType::ScriptRequest(ScriptRequest {
                script: dev_server,
                language: ScriptRequestLanguage::Bash,
                context: ScriptContext::DevServer,
            }),
            None,
        );

        deployment
            .container()
            .start_execution(
                &task_attempt,
                &executor_action,
                &ExecutionProcessRunReason::DevServer,
            )
            .await?
    } else {
        return Ok(ResponseJson(ApiResponse::error(
            "No dev server script configured for this project",
        )));
    };

    Ok(ResponseJson(ApiResponse::success(())))
}

// ============================================================================
// Children and Stop
// ============================================================================

pub async fn get_task_attempt_children(
    Extension(task_attempt): Extension<TaskAttempt>,
    remote_ctx: Option<Extension<RemoteTaskAttemptContext>>,
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<TaskRelationships>>, ApiError> {
    // For remote tasks, relationships are stored on the Hive - return empty
    if remote_ctx.is_some() {
        return Ok(ResponseJson(ApiResponse::success(TaskRelationships {
            parent_task: None,
            current_attempt: task_attempt,
            children: vec![],
        })));
    }

    let relationships =
        Task::find_relationships_for_attempt(&deployment.db().pool, &task_attempt).await?;
    Ok(ResponseJson(ApiResponse::success(relationships)))
}

pub async fn stop_task_attempt_execution(
    Extension(task_attempt): Extension<TaskAttempt>,
    remote_ctx: Option<Extension<RemoteTaskAttemptContext>>,
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<()>>, ApiError> {
    // Check if this is a remote task attempt that should be proxied
    if let Some(proxy_info) = check_remote_task_attempt_proxy(remote_ctx.as_ref().map(|e| &e.0))? {
        tracing::debug!(
            node_id = %proxy_info.node_id,
            shared_task_id = %proxy_info.target_id,
            "Proxying stop_task_attempt_execution to remote node"
        );

        let path = format!("/task-attempts/by-task-id/{}/stop", proxy_info.target_id);
        let response: ApiResponse<()> = deployment
            .node_proxy_client()
            .proxy_post(&proxy_info.node_url, &path, &(), proxy_info.node_id)
            .await?;

        return Ok(ResponseJson(response));
    }

    deployment.container().try_stop(&task_attempt).await;

    Ok(ResponseJson(ApiResponse::success(())))
}

// ============================================================================
// Session Error Handling
// ============================================================================

/// Fix corrupted sessions by invalidating sessions from failed/killed execution processes
pub async fn fix_sessions(
    Extension(task_attempt): Extension<TaskAttempt>,
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<FixSessionsResponse>>, ApiError> {
    let pool = &deployment.db().pool;

    let invalidated = ExecutorSession::invalidate_failed_sessions(pool, task_attempt.id).await?;

    tracing::info!(
        task_attempt_id = %task_attempt.id,
        invalidated_count = invalidated.len(),
        "Fixed corrupted sessions for task attempt"
    );

    Ok(ResponseJson(ApiResponse::success(FixSessionsResponse {
        invalidated_count: invalidated.len(),
        invalidated_session_ids: invalidated,
    })))
}

/// Check if the latest failed execution has a session invalid error that can be fixed.
/// Returns true only if:
/// 1. The latest CodingAgent execution failed with a session error, AND
/// 2. There are sessions from failed processes that can still be invalidated
pub async fn has_session_error(
    Extension(task_attempt): Extension<TaskAttempt>,
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<bool>>, ApiError> {
    let pool = &deployment.db().pool;

    // Get latest coding agent execution
    let latest = ExecutionProcess::find_latest_by_task_attempt_and_run_reason(
        pool,
        task_attempt.id,
        &ExecutionProcessRunReason::CodingAgent,
    )
    .await?;

    let has_error = if let Some(exec) = latest {
        if exec.status == ExecutionProcessStatus::Failed {
            // Check if logs contain session error
            let has_session_error_in_logs =
                ExecutionProcessLogs::contains_session_invalid_error(pool, exec.id)
                    .await
                    .unwrap_or(false);

            if has_session_error_in_logs {
                // Also check if there are sessions that can be fixed
                // (i.e., sessions from failed processes that still have session_id set)
                let fixable_sessions =
                    ExecutorSession::count_fixable_sessions(pool, task_attempt.id).await?;
                fixable_sessions > 0
            } else {
                false
            }
        } else {
            false
        }
    } else {
        false
    };

    Ok(ResponseJson(ApiResponse::success(has_error)))
}
