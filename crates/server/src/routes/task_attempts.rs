pub mod codex_setup;
pub mod cursor_setup;
pub mod drafts;
pub mod gh_cli_setup;
pub mod util;

use axum::{
    Extension, Json, Router,
    extract::{
        Path, Query, State,
        ws::{WebSocket, WebSocketUpgrade},
    },
    http::StatusCode,
    middleware::from_fn_with_state,
    response::{IntoResponse, Json as ResponseJson},
    routing::{get, post},
};
use db::models::{
    draft::{Draft, DraftType},
    execution_process::{ExecutionProcess, ExecutionProcessRunReason, ExecutionProcessStatus},
    merge::{Merge, MergeStatus, PrMerge, PullRequestInfo},
    project::{Project, ProjectError},
    task::{Task, TaskRelationships, TaskStatus},
    task_attempt::{CreateTaskAttempt, TaskAttempt, TaskAttemptError},
    task_variable::TaskVariable,
};
use deployment::Deployment;
use executors::{
    actions::{
        ExecutorAction, ExecutorActionType,
        coding_agent_follow_up::CodingAgentFollowUpRequest,
        script::{ScriptContext, ScriptRequest, ScriptRequestLanguage},
    },
    executors::{CodingAgent, ExecutorError},
    profile::{ExecutorConfigs, ExecutorProfileId},
};
use git2::BranchType;
use serde::{Deserialize, Serialize};
use services::services::{
    container::ContainerService,
    filesystem::{DirectoryListResponse, FileContentResponse, FilesystemError},
    git::{ConflictOp, GitCliError, GitServiceError, WorktreeResetOptions},
    github::{CreatePrRequest, GitHubService, GitHubServiceError},
    variable_expander,
};
use sqlx::Error as SqlxError;
use ts_rs::TS;
use utils::response::ApiResponse;
use uuid::Uuid;

use crate::{
    DeploymentImpl,
    error::ApiError,
    middleware::{
        RemoteTaskAttemptContext, load_task_attempt_by_task_id_middleware,
        load_task_attempt_by_task_id_middleware_with_wildcard, load_task_attempt_middleware,
        load_task_attempt_middleware_with_wildcard, load_task_by_task_id_middleware,
    },
    routes::task_attempts::{
        gh_cli_setup::GhCliSetupError,
        util::{ensure_worktree_path, handle_images_for_prompt},
    },
};

/// Helper to check if a remote task attempt context is available and online.
/// Returns Some((node_url, node_id, shared_task_id)) if we should proxy,
/// or an Err if the remote node is offline.
fn check_remote_task_attempt_proxy(
    remote_ctx: Option<&RemoteTaskAttemptContext>,
) -> Result<Option<(String, Uuid, Uuid)>, ApiError> {
    match remote_ctx {
        Some(ctx) => {
            // Check if the node is online
            if ctx.node_status.as_deref() != Some("online") {
                return Err(ApiError::BadGateway(format!(
                    "Remote node '{}' is offline",
                    ctx.node_id
                )));
            }

            // Check if we have a URL to proxy to
            let node_url = ctx.node_url.as_ref().ok_or_else(|| {
                ApiError::BadGateway(format!(
                    "Remote node '{}' has no public URL configured",
                    ctx.node_id
                ))
            })?;

            Ok(Some((node_url.clone(), ctx.node_id, ctx.task_id)))
        }
        None => Ok(None),
    }
}

#[derive(Debug, Deserialize, Serialize, TS)]
pub struct RebaseTaskAttemptRequest {
    pub old_base_branch: Option<String>,
    pub new_base_branch: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
#[ts(tag = "type", rename_all = "snake_case")]
pub enum GitOperationError {
    MergeConflicts { message: String, op: ConflictOp },
    RebaseInProgress,
}

#[derive(Debug, Deserialize, Serialize, TS)]
pub struct CreateGitHubPrRequest {
    pub title: String,
    pub body: Option<String>,
    pub target_branch: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct TaskAttemptQuery {
    pub task_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
pub struct DiffStreamQuery {
    #[serde(default)]
    pub stats_only: bool,
}

pub async fn get_task_attempts(
    State(deployment): State<DeploymentImpl>,
    Query(query): Query<TaskAttemptQuery>,
) -> Result<ResponseJson<ApiResponse<Vec<TaskAttempt>>>, ApiError> {
    let pool = &deployment.db().pool;
    let attempts = TaskAttempt::fetch_all(pool, query.task_id).await?;
    Ok(ResponseJson(ApiResponse::success(attempts)))
}

pub async fn get_task_attempt(
    Extension(task_attempt): Extension<TaskAttempt>,
    State(_deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<TaskAttempt>>, ApiError> {
    Ok(ResponseJson(ApiResponse::success(task_attempt)))
}

#[derive(Debug, Serialize, Deserialize, ts_rs::TS)]
pub struct CreateTaskAttemptBody {
    pub task_id: Uuid,
    /// Executor profile specification
    pub executor_profile_id: ExecutorProfileId,
    pub base_branch: String,
    /// Target node ID for remote execution (if project exists on multiple nodes).
    /// When set, the request will be proxied to the specified node.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_node_id: Option<Uuid>,
    /// When true, reuse the parent task's latest attempt worktree.
    /// Only valid when the task has a parent_task_id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub use_parent_worktree: Option<bool>,
}

impl CreateTaskAttemptBody {
    /// Get the executor profile ID
    pub fn get_executor_profile_id(&self) -> ExecutorProfileId {
        self.executor_profile_id.clone()
    }
}

/// Request body for creating a task attempt via by-task-id route (cross-node proxying).
/// Unlike CreateTaskAttemptBody, this doesn't need task_id since it's in the URL path.
#[derive(Debug, Serialize, Deserialize)]
pub struct CreateTaskAttemptByTaskIdBody {
    /// Executor profile specification
    pub executor_profile_id: ExecutorProfileId,
    pub base_branch: String,
    /// When true, reuse the parent task's latest attempt worktree.
    /// Only valid when the task has a parent_task_id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub use_parent_worktree: Option<bool>,
}

#[derive(Debug, Deserialize, Serialize, TS)]
pub struct RunAgentSetupRequest {
    pub executor_profile_id: ExecutorProfileId,
}

#[derive(Debug, Serialize, TS)]
pub struct RunAgentSetupResponse {}

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

        return Ok(ResponseJson(response));
    }

    // Local execution path
    let executor_profile_id = payload.get_executor_profile_id();
    let pool = &deployment.db().pool;
    let task = Task::find_by_id(pool, payload.task_id)
        .await?
        .ok_or(SqlxError::RowNotFound)?;

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

#[derive(Debug, Deserialize, Serialize, TS)]
pub struct CreateFollowUpAttempt {
    pub prompt: String,
    pub variant: Option<String>,
    pub image_ids: Option<Vec<Uuid>>,
    pub retry_process_id: Option<Uuid>,
    pub force_when_dirty: Option<bool>,
    pub perform_git_reset: Option<bool>,
}

pub async fn follow_up(
    Extension(task_attempt): Extension<TaskAttempt>,
    remote_ctx: Option<Extension<RemoteTaskAttemptContext>>,
    State(deployment): State<DeploymentImpl>,
    Json(payload): Json<CreateFollowUpAttempt>,
) -> Result<ResponseJson<ApiResponse<ExecutionProcess>>, ApiError> {
    // Check if this is a remote task attempt that should be proxied
    if let Some((node_url, node_id, shared_task_id)) =
        check_remote_task_attempt_proxy(remote_ctx.as_ref().map(|e| &e.0))?
    {
        tracing::debug!(
            node_id = %node_id,
            shared_task_id = %shared_task_id,
            "Proxying follow_up to remote node"
        );

        let path = format!("/task-attempts/by-task-id/{}/follow-up", shared_task_id);
        let response: ApiResponse<ExecutionProcess> = deployment
            .node_proxy_client()
            .proxy_post(&node_url, &path, &payload, node_id)
            .await?;

        return Ok(ResponseJson(response));
    }

    tracing::info!("{:?}", task_attempt);

    // Ensure worktree exists (recreate if needed for cold task support)
    let _ = ensure_worktree_path(&deployment, &task_attempt).await?;

    // Get executor profile data from the latest CodingAgent process
    let initial_executor_profile_id = ExecutionProcess::latest_executor_profile_for_attempt(
        &deployment.db().pool,
        task_attempt.id,
    )
    .await?;

    let executor_profile_id = ExecutorProfileId {
        executor: initial_executor_profile_id.executor,
        variant: payload.variant,
    };

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

    // If retry settings provided, perform replace-logic before proceeding
    if let Some(proc_id) = payload.retry_process_id {
        let pool = &deployment.db().pool;
        // Validate process belongs to attempt
        let process =
            ExecutionProcess::find_by_id(pool, proc_id)
                .await?
                .ok_or(ApiError::TaskAttempt(TaskAttemptError::ValidationError(
                    "Process not found".to_string(),
                )))?;
        if process.task_attempt_id != task_attempt.id {
            return Err(ApiError::TaskAttempt(TaskAttemptError::ValidationError(
                "Process does not belong to this attempt".to_string(),
            )));
        }

        // Determine target reset OID: before the target process
        let mut target_before_oid = process.before_head_commit.clone();
        if target_before_oid.is_none() {
            target_before_oid =
                ExecutionProcess::find_prev_after_head_commit(pool, task_attempt.id, proc_id)
                    .await?;
        }

        // Decide if Git reset is needed and apply it (best-effort)
        let force_when_dirty = payload.force_when_dirty.unwrap_or(false);
        let perform_git_reset = payload.perform_git_reset.unwrap_or(true);
        if let Some(target_oid) = &target_before_oid {
            let wt_buf = ensure_worktree_path(&deployment, &task_attempt).await?;
            let wt = wt_buf.as_path();
            let is_dirty = deployment
                .container()
                .is_container_clean(&task_attempt)
                .await
                .map(|is_clean| !is_clean)
                .unwrap_or(false);

            deployment.git().reconcile_worktree_to_commit(
                wt,
                target_oid,
                WorktreeResetOptions::new(
                    perform_git_reset,
                    force_when_dirty,
                    is_dirty,
                    perform_git_reset,
                ),
            );
        }

        // Stop any running processes for this attempt
        deployment.container().try_stop(&task_attempt).await;

        // Soft-drop the target process and all later processes
        let _ = ExecutionProcess::drop_at_and_after(pool, task_attempt.id, proc_id).await?;

        // Best-effort: clear any retry draft for this attempt
        let _ = Draft::clear_after_send(pool, task_attempt.id, DraftType::Retry).await;
    }

    // Check if the selected executor profile has no_context enabled
    let executor_configs = ExecutorConfigs::get_cached();
    let coding_agent = executor_configs.get_coding_agent_or_default(&executor_profile_id);
    let skip_context = coding_agent.no_context();

    // If no_context is enabled, skip session lookup and start fresh
    let latest_session_id = if skip_context {
        None
    } else {
        ExecutionProcess::find_latest_session_id_by_task_attempt(
            &deployment.db().pool,
            task_attempt.id,
        )
        .await?
    };

    let mut prompt = payload.prompt;
    if let Some(image_ids) = &payload.image_ids {
        prompt = handle_images_for_prompt(&deployment, &task_attempt, task.id, image_ids, &prompt)
            .await?;
    }

    // Expand task variables ($VAR and ${VAR} syntax) in follow-up prompt
    let prompt = {
        let variables = TaskVariable::get_variable_map(&deployment.db().pool, task.id)
            .await
            .unwrap_or_else(|e| {
                tracing::warn!(task_id = %task.id, error = ?e, "Failed to fetch task variables for follow-up");
                std::collections::HashMap::new()
            });

        if variables.is_empty() {
            prompt
        } else {
            let variables: std::collections::HashMap<String, (String, Option<Uuid>)> = variables
                .into_iter()
                .map(|(k, (v, id))| (k, (v, Some(id))))
                .collect();

            let result = variable_expander::expand_variables(&prompt, &variables);

            if !result.undefined_vars.is_empty() {
                tracing::warn!(
                    task_id = %task.id,
                    undefined_vars = ?result.undefined_vars,
                    "Follow-up prompt contains undefined variables"
                );
            }

            if !result.expanded_vars.is_empty() {
                tracing::info!(
                    task_id = %task.id,
                    expanded_count = result.expanded_vars.len(),
                    "Expanded task variables in follow-up prompt"
                );
            }

            result.text
        }
    };

    let cleanup_action = deployment
        .container()
        .cleanup_action(project.cleanup_script);

    let action_type = if let Some(session_id) = latest_session_id {
        ExecutorActionType::CodingAgentFollowUpRequest(CodingAgentFollowUpRequest {
            prompt: prompt.clone(),
            session_id,
            executor_profile_id: executor_profile_id.clone(),
        })
    } else {
        ExecutorActionType::CodingAgentInitialRequest(
            executors::actions::coding_agent_initial::CodingAgentInitialRequest {
                prompt,
                executor_profile_id: executor_profile_id.clone(),
            },
        )
    };

    let action = ExecutorAction::new(action_type, cleanup_action);

    let execution_process = deployment
        .container()
        .start_execution(
            &task_attempt,
            &action,
            &ExecutionProcessRunReason::CodingAgent,
        )
        .await?;

    // Clear drafts post-send:
    // - If this was a retry send, the retry draft has already been cleared above.
    // - Otherwise, clear the follow-up draft to avoid.
    if payload.retry_process_id.is_none() {
        let _ =
            Draft::clear_after_send(&deployment.db().pool, task_attempt.id, DraftType::FollowUp)
                .await;
    }

    Ok(ResponseJson(ApiResponse::success(execution_process)))
}

#[axum::debug_handler]
pub async fn stream_task_attempt_diff_ws(
    ws: WebSocketUpgrade,
    Query(params): Query<DiffStreamQuery>,
    Extension(task_attempt): Extension<TaskAttempt>,
    State(deployment): State<DeploymentImpl>,
) -> impl IntoResponse {
    let stats_only = params.stats_only;
    ws.on_upgrade(move |socket| async move {
        if let Err(e) =
            handle_task_attempt_diff_ws(socket, deployment, task_attempt, stats_only).await
        {
            tracing::warn!("diff WS closed: {}", e);
        }
    })
}

async fn handle_task_attempt_diff_ws(
    socket: WebSocket,
    deployment: DeploymentImpl,
    task_attempt: TaskAttempt,
    stats_only: bool,
) -> anyhow::Result<()> {
    use crate::ws_util::{WsKeepAlive, run_ws_stream};
    use futures_util::TryStreamExt;
    use utils::log_msg::LogMsg;

    let stream = deployment
        .container()
        .stream_diff(&task_attempt, stats_only)
        .await?;

    let stream = stream.map_ok(|msg: LogMsg| msg.to_ws_message_unchecked());

    // Use run_ws_stream for proper keep-alive handling
    run_ws_stream(socket, stream, WsKeepAlive::for_execution_streams()).await
}

#[derive(Debug, Serialize, TS)]
pub struct CommitInfo {
    pub sha: String,
    pub subject: String,
}

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

#[derive(Debug, Serialize, TS)]
pub struct CommitCompareResult {
    pub head_oid: String,
    pub target_oid: String,
    pub ahead_from_head: usize,
    pub behind_from_head: usize,
    pub is_linear: bool,
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

#[axum::debug_handler]
pub async fn merge_task_attempt(
    Extension(task_attempt): Extension<TaskAttempt>,
    remote_ctx: Option<Extension<RemoteTaskAttemptContext>>,
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<()>>, ApiError> {
    // Check if this is a remote task attempt that should be proxied
    if let Some((node_url, node_id, shared_task_id)) =
        check_remote_task_attempt_proxy(remote_ctx.as_ref().map(|e| &e.0))?
    {
        tracing::debug!(
            node_id = %node_id,
            shared_task_id = %shared_task_id,
            "Proxying merge_task_attempt to remote node"
        );

        let path = format!("/task-attempts/by-task-id/{}/merge", shared_task_id);
        let response: ApiResponse<()> = deployment
            .node_proxy_client()
            .proxy_post(&node_url, &path, &(), node_id)
            .await?;

        return Ok(ResponseJson(response));
    }

    let pool = &deployment.db().pool;

    let task = task_attempt
        .parent_task(pool)
        .await?
        .ok_or(ApiError::TaskAttempt(TaskAttemptError::TaskNotFound))?;
    let ctx = TaskAttempt::load_context(pool, task_attempt.id, task.id, task.project_id).await?;

    let worktree_path_buf = ensure_worktree_path(&deployment, &task_attempt).await?;
    let worktree_path = worktree_path_buf.as_path();

    let task_uuid_str = task.id.to_string();
    let first_uuid_section = task_uuid_str.split('-').next().unwrap_or(&task_uuid_str);

    // Create commit message with task title and description
    let mut commit_message = format!("{} (vibe-kanban {})", ctx.task.title, first_uuid_section);

    // Add description on next line if it exists
    if let Some(description) = &ctx.task.description
        && !description.trim().is_empty()
    {
        commit_message.push_str("\n\n");
        commit_message.push_str(description);
    }

    let merge_commit_id = deployment.git().merge_changes(
        &ctx.project.git_repo_path,
        worktree_path,
        &ctx.task_attempt.branch,
        &ctx.task_attempt.target_branch,
        &commit_message,
    )?;

    Merge::create_direct(
        pool,
        task_attempt.id,
        &ctx.task_attempt.target_branch,
        &merge_commit_id,
    )
    .await?;
    Task::update_status(pool, ctx.task.id, TaskStatus::Done).await?;

    // Stop any running dev servers for this task attempt
    let dev_servers =
        ExecutionProcess::find_running_dev_servers_by_task_attempt(pool, task_attempt.id).await?;

    for dev_server in dev_servers {
        tracing::info!(
            "Stopping dev server {} for completed task attempt {}",
            dev_server.id,
            task_attempt.id
        );

        if let Err(e) = deployment
            .container()
            .stop_execution(&dev_server, ExecutionProcessStatus::Killed)
            .await
        {
            tracing::error!(
                "Failed to stop dev server {} for task attempt {}: {}",
                dev_server.id,
                task_attempt.id,
                e
            );
        }
    }

    // Try broadcast update to other users in organization
    if let Ok(publisher) = deployment.share_publisher() {
        if let Err(err) = publisher.update_shared_task_by_id(ctx.task.id).await {
            tracing::warn!(
                ?err,
                "Failed to propagate shared task update for {}",
                ctx.task.id
            );
        }
    } else {
        tracing::debug!(
            "Share publisher unavailable; skipping remote update for {}",
            ctx.task.id
        );
    }

    Ok(ResponseJson(ApiResponse::success(())))
}

pub async fn push_task_attempt_branch(
    Extension(task_attempt): Extension<TaskAttempt>,
    remote_ctx: Option<Extension<RemoteTaskAttemptContext>>,
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<(), PushError>>, ApiError> {
    // Check if this is a remote task attempt that should be proxied
    if let Some((node_url, node_id, shared_task_id)) =
        check_remote_task_attempt_proxy(remote_ctx.as_ref().map(|e| &e.0))?
    {
        tracing::debug!(
            node_id = %node_id,
            shared_task_id = %shared_task_id,
            "Proxying push_task_attempt_branch to remote node"
        );

        let path = format!("/task-attempts/by-task-id/{}/push", shared_task_id);
        let response: ApiResponse<(), PushError> = deployment
            .node_proxy_client()
            .proxy_post(&node_url, &path, &(), node_id)
            .await?;

        return Ok(ResponseJson(response));
    }

    let github_service = GitHubService::new()?;
    github_service.check_token().await?;

    let ws_path = ensure_worktree_path(&deployment, &task_attempt).await?;

    match deployment
        .git()
        .push_to_github(&ws_path, &task_attempt.branch, false)
    {
        Ok(_) => Ok(ResponseJson(ApiResponse::success(()))),
        Err(GitServiceError::GitCLI(GitCliError::PushRejected(_))) => Ok(ResponseJson(
            ApiResponse::error_with_data(PushError::ForcePushRequired),
        )),
        Err(e) => Err(ApiError::GitService(e)),
    }
}

pub async fn force_push_task_attempt_branch(
    Extension(task_attempt): Extension<TaskAttempt>,
    remote_ctx: Option<Extension<RemoteTaskAttemptContext>>,
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<(), PushError>>, ApiError> {
    // Check if this is a remote task attempt that should be proxied
    if let Some((node_url, node_id, shared_task_id)) =
        check_remote_task_attempt_proxy(remote_ctx.as_ref().map(|e| &e.0))?
    {
        tracing::debug!(
            node_id = %node_id,
            shared_task_id = %shared_task_id,
            "Proxying force_push_task_attempt_branch to remote node"
        );

        let path = format!("/task-attempts/by-task-id/{}/push/force", shared_task_id);
        let response: ApiResponse<(), PushError> = deployment
            .node_proxy_client()
            .proxy_post(&node_url, &path, &(), node_id)
            .await?;

        return Ok(ResponseJson(response));
    }

    let github_service = GitHubService::new()?;
    github_service.check_token().await?;

    let ws_path = ensure_worktree_path(&deployment, &task_attempt).await?;

    deployment
        .git()
        .push_to_github(&ws_path, &task_attempt.branch, true)?;
    Ok(ResponseJson(ApiResponse::success(())))
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
#[ts(tag = "type", rename_all = "snake_case")]
pub enum PushError {
    ForcePushRequired,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
#[ts(tag = "type", rename_all = "snake_case")]
pub enum CreatePrError {
    GithubCliNotInstalled,
    GithubCliNotLoggedIn,
    GitCliNotLoggedIn,
    GitCliNotInstalled,
    TargetBranchNotFound { branch: String },
}

pub async fn create_github_pr(
    Extension(task_attempt): Extension<TaskAttempt>,
    remote_ctx: Option<Extension<RemoteTaskAttemptContext>>,
    State(deployment): State<DeploymentImpl>,
    Json(request): Json<CreateGitHubPrRequest>,
) -> Result<ResponseJson<ApiResponse<String, CreatePrError>>, ApiError> {
    // Check if this is a remote task attempt that should be proxied
    if let Some((node_url, node_id, shared_task_id)) =
        check_remote_task_attempt_proxy(remote_ctx.as_ref().map(|e| &e.0))?
    {
        tracing::debug!(
            node_id = %node_id,
            shared_task_id = %shared_task_id,
            "Proxying create_github_pr to remote node"
        );

        let path = format!("/task-attempts/by-task-id/{}/pr", shared_task_id);
        let response: ApiResponse<String, CreatePrError> = deployment
            .node_proxy_client()
            .proxy_post(&node_url, &path, &request, node_id)
            .await?;

        return Ok(ResponseJson(response));
    }

    let github_config = deployment.config().read().await.github.clone();
    // Get the task attempt to access the stored target branch
    let target_branch = request.target_branch.unwrap_or_else(|| {
        // Use the stored target branch from the task attempt as the default
        // Fall back to config default or "main" only if stored target branch is somehow invalid
        if !task_attempt.target_branch.trim().is_empty() {
            task_attempt.target_branch.clone()
        } else {
            github_config
                .default_pr_base
                .as_ref()
                .map_or_else(|| "main".to_string(), |b| b.to_string())
        }
    });

    let pool = &deployment.db().pool;
    let task = task_attempt
        .parent_task(pool)
        .await?
        .ok_or(ApiError::TaskAttempt(TaskAttemptError::TaskNotFound))?;
    let project = Project::find_by_id(pool, task.project_id)
        .await?
        .ok_or(ApiError::Project(ProjectError::ProjectNotFound))?;

    let workspace_path = ensure_worktree_path(&deployment, &task_attempt).await?;

    match deployment
        .git()
        .check_remote_branch_exists(&project.git_repo_path, &target_branch)
    {
        Ok(false) => {
            return Ok(ResponseJson(ApiResponse::error_with_data(
                CreatePrError::TargetBranchNotFound {
                    branch: target_branch.clone(),
                },
            )));
        }
        Err(GitServiceError::GitCLI(GitCliError::AuthFailed(_))) => {
            return Ok(ResponseJson(ApiResponse::error_with_data(
                CreatePrError::GitCliNotLoggedIn,
            )));
        }
        Err(GitServiceError::GitCLI(GitCliError::NotAvailable)) => {
            return Ok(ResponseJson(ApiResponse::error_with_data(
                CreatePrError::GitCliNotInstalled,
            )));
        }
        Err(e) => return Err(ApiError::GitService(e)),
        Ok(true) => {}
    }

    // Push the branch to GitHub first
    if let Err(e) = deployment
        .git()
        .push_to_github(&workspace_path, &task_attempt.branch, false)
    {
        tracing::error!("Failed to push branch to GitHub: {}", e);
        match e {
            GitServiceError::GitCLI(GitCliError::AuthFailed(_)) => {
                return Ok(ResponseJson(ApiResponse::error_with_data(
                    CreatePrError::GitCliNotLoggedIn,
                )));
            }
            GitServiceError::GitCLI(GitCliError::NotAvailable) => {
                return Ok(ResponseJson(ApiResponse::error_with_data(
                    CreatePrError::GitCliNotInstalled,
                )));
            }
            _ => return Err(ApiError::GitService(e)),
        }
    }

    let norm_target_branch_name = if matches!(
        deployment
            .git()
            .find_branch_type(&project.git_repo_path, &target_branch)?,
        BranchType::Remote
    ) {
        // Remote branches are formatted as {remote}/{branch} locally.
        // For PR APIs, we must provide just the branch name.
        let remote = deployment
            .git()
            .get_remote_name_from_branch_name(&workspace_path, &target_branch)?;
        let remote_prefix = format!("{}/", remote);
        target_branch
            .strip_prefix(&remote_prefix)
            .unwrap_or(&target_branch)
            .to_string()
    } else {
        target_branch
    };
    // Create the PR using GitHub service
    let pr_request = CreatePrRequest {
        title: request.title.clone(),
        body: request.body.clone(),
        head_branch: task_attempt.branch.clone(),
        base_branch: norm_target_branch_name.clone(),
    };
    // Use GitService to get the remote URL, then create GitHubRepoInfo
    let repo_info = deployment
        .git()
        .get_github_repo_info(&project.git_repo_path)?;

    // Use GitHubService to create the PR
    let github_service = GitHubService::new()?;
    match github_service.create_pr(&repo_info, &pr_request).await {
        Ok(pr_info) => {
            // Update the task attempt with PR information
            if let Err(e) = Merge::create_pr(
                pool,
                task_attempt.id,
                &norm_target_branch_name,
                pr_info.number,
                &pr_info.url,
            )
            .await
            {
                tracing::error!("Failed to update task attempt PR status: {}", e);
            }

            // Auto-open PR in browser
            if let Err(e) = utils::browser::open_browser(&pr_info.url).await {
                tracing::warn!("Failed to open PR in browser: {}", e);
            }

            Ok(ResponseJson(ApiResponse::success(pr_info.url)))
        }
        Err(e) => {
            tracing::error!(
                "Failed to create GitHub PR for attempt {}: {}",
                task_attempt.id,
                e
            );
            match &e {
                GitHubServiceError::GhCliNotInstalled(_) => Ok(ResponseJson(
                    ApiResponse::error_with_data(CreatePrError::GithubCliNotInstalled),
                )),
                GitHubServiceError::AuthFailed(_) => Ok(ResponseJson(
                    ApiResponse::error_with_data(CreatePrError::GithubCliNotLoggedIn),
                )),
                _ => Err(ApiError::GitHubService(e)),
            }
        }
    }
}

#[derive(serde::Deserialize, TS)]
pub struct OpenEditorRequest {
    editor_type: Option<String>,
    file_path: Option<String>,
}

#[derive(Debug, Serialize, TS)]
pub struct OpenEditorResponse {
    pub url: Option<String>,
}

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

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct BranchStatus {
    pub commits_behind: Option<usize>,
    pub commits_ahead: Option<usize>,
    pub has_uncommitted_changes: Option<bool>,
    pub head_oid: Option<String>,
    pub uncommitted_count: Option<usize>,
    pub untracked_count: Option<usize>,
    pub target_branch_name: String,
    pub remote_commits_behind: Option<usize>,
    pub remote_commits_ahead: Option<usize>,
    pub merges: Vec<Merge>,
    /// True if a `git rebase` is currently in progress in this worktree
    pub is_rebase_in_progress: bool,
    /// Current conflict operation if any
    pub conflict_op: Option<ConflictOp>,
    /// List of files currently in conflicted (unmerged) state
    pub conflicted_files: Vec<String>,
}

pub async fn get_task_attempt_branch_status(
    Extension(task_attempt): Extension<TaskAttempt>,
    remote_ctx: Option<Extension<RemoteTaskAttemptContext>>,
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<BranchStatus>>, ApiError> {
    // Check if this is a remote task attempt that should be proxied
    if let Some((node_url, node_id, shared_task_id)) =
        check_remote_task_attempt_proxy(remote_ctx.as_ref().map(|e| &e.0))?
    {
        tracing::debug!(
            node_id = %node_id,
            shared_task_id = %shared_task_id,
            "Proxying get_task_attempt_branch_status to remote node"
        );

        let path = format!("/task-attempts/by-task-id/{}/branch-status", shared_task_id);
        let response: ApiResponse<BranchStatus> = deployment
            .node_proxy_client()
            .proxy_get(&node_url, &path, node_id)
            .await?;

        return Ok(ResponseJson(response));
    }

    let pool = &deployment.db().pool;

    let task = task_attempt
        .parent_task(pool)
        .await?
        .ok_or(ApiError::TaskAttempt(TaskAttemptError::TaskNotFound))?;
    let ctx = TaskAttempt::load_context(pool, task_attempt.id, task.id, task.project_id).await?;
    let has_uncommitted_changes = deployment
        .container()
        .is_container_clean(&task_attempt)
        .await
        .ok()
        .map(|is_clean| !is_clean);
    let head_oid = {
        let wt_buf = ensure_worktree_path(&deployment, &task_attempt).await?;
        let wt = wt_buf.as_path();
        deployment.git().get_head_info(wt).ok().map(|h| h.oid)
    };
    // Detect conflicts and operation in progress (best-effort)
    let (is_rebase_in_progress, conflicted_files, conflict_op) = {
        let wt_buf = ensure_worktree_path(&deployment, &task_attempt).await?;
        let wt = wt_buf.as_path();
        let in_rebase = deployment.git().is_rebase_in_progress(wt).unwrap_or(false);
        let conflicts = deployment
            .git()
            .get_conflicted_files(wt)
            .unwrap_or_default();
        let op = if conflicts.is_empty() {
            None
        } else {
            deployment.git().detect_conflict_op(wt).unwrap_or(None)
        };
        (in_rebase, conflicts, op)
    };
    let (uncommitted_count, untracked_count) = {
        let wt_buf = ensure_worktree_path(&deployment, &task_attempt).await?;
        let wt = wt_buf.as_path();
        match deployment.git().get_worktree_change_counts(wt) {
            Ok((a, b)) => (Some(a), Some(b)),
            Err(_) => (None, None),
        }
    };

    let target_branch_type = deployment
        .git()
        .find_branch_type(&ctx.project.git_repo_path, &task_attempt.target_branch)?;

    let (commits_ahead, commits_behind) = match target_branch_type {
        BranchType::Local => {
            let (a, b) = deployment.git().get_branch_status(
                &ctx.project.git_repo_path,
                &task_attempt.branch,
                &task_attempt.target_branch,
            )?;
            (Some(a), Some(b))
        }
        BranchType::Remote => {
            let (remote_commits_ahead, remote_commits_behind) =
                deployment.git().get_remote_branch_status(
                    &ctx.project.git_repo_path,
                    &task_attempt.branch,
                    Some(&task_attempt.target_branch),
                )?;
            (Some(remote_commits_ahead), Some(remote_commits_behind))
        }
    };
    // Fetch merges for this task attempt and add to branch status
    let merges = Merge::find_by_task_attempt_id(pool, task_attempt.id).await?;
    let (remote_ahead, remote_behind) = if let Some(Merge::Pr(PrMerge {
        pr_info: PullRequestInfo {
            status: MergeStatus::Open,
            ..
        },
        ..
    })) = merges.first()
    {
        // check remote status if the attempt has an open PR
        let (remote_commits_ahead, remote_commits_behind) = deployment
            .git()
            .get_remote_branch_status(&ctx.project.git_repo_path, &task_attempt.branch, None)?;
        (Some(remote_commits_ahead), Some(remote_commits_behind))
    } else {
        (None, None)
    };

    let branch_status = BranchStatus {
        commits_ahead,
        commits_behind,
        has_uncommitted_changes,
        head_oid,
        uncommitted_count,
        untracked_count,
        remote_commits_ahead: remote_ahead,
        remote_commits_behind: remote_behind,
        merges,
        target_branch_name: task_attempt.target_branch,
        is_rebase_in_progress,
        conflict_op,
        conflicted_files,
    };
    Ok(ResponseJson(ApiResponse::success(branch_status)))
}

#[derive(serde::Deserialize, serde::Serialize, Debug, TS)]
pub struct ChangeTargetBranchRequest {
    pub new_target_branch: String,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, TS)]
pub struct ChangeTargetBranchResponse {
    pub new_target_branch: String,
    pub status: (usize, usize),
}

#[derive(serde::Deserialize, serde::Serialize, Debug, TS)]
pub struct RenameBranchRequest {
    pub new_branch_name: String,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, TS)]
pub struct RenameBranchResponse {
    pub branch: String,
}

#[axum::debug_handler]
pub async fn change_target_branch(
    Extension(task_attempt): Extension<TaskAttempt>,
    remote_ctx: Option<Extension<RemoteTaskAttemptContext>>,
    State(deployment): State<DeploymentImpl>,
    Json(payload): Json<ChangeTargetBranchRequest>,
) -> Result<ResponseJson<ApiResponse<ChangeTargetBranchResponse>>, ApiError> {
    // Check if this is a remote task attempt that should be proxied
    if let Some((node_url, node_id, shared_task_id)) =
        check_remote_task_attempt_proxy(remote_ctx.as_ref().map(|e| &e.0))?
    {
        tracing::debug!(
            node_id = %node_id,
            shared_task_id = %shared_task_id,
            "Proxying change_target_branch to remote node"
        );

        let path = format!(
            "/task-attempts/by-task-id/{}/change-target-branch",
            shared_task_id
        );
        let response: ApiResponse<ChangeTargetBranchResponse> = deployment
            .node_proxy_client()
            .proxy_post(&node_url, &path, &payload, node_id)
            .await?;

        return Ok(ResponseJson(response));
    }

    // Extract new base branch from request body if provided
    let new_target_branch = payload.new_target_branch;
    let task = task_attempt
        .parent_task(&deployment.db().pool)
        .await?
        .ok_or(ApiError::TaskAttempt(TaskAttemptError::TaskNotFound))?;
    let project = Project::find_by_id(&deployment.db().pool, task.project_id)
        .await?
        .ok_or(ApiError::Project(ProjectError::ProjectNotFound))?;
    match deployment
        .git()
        .check_branch_exists(&project.git_repo_path, &new_target_branch)?
    {
        true => {
            TaskAttempt::update_target_branch(
                &deployment.db().pool,
                task_attempt.id,
                &new_target_branch,
            )
            .await?;
        }
        false => {
            return Ok(ResponseJson(ApiResponse::error(
                format!(
                    "Branch '{}' does not exist in the repository",
                    new_target_branch
                )
                .as_str(),
            )));
        }
    }
    let status = deployment.git().get_branch_status(
        &project.git_repo_path,
        &task_attempt.branch,
        &new_target_branch,
    )?;

    Ok(ResponseJson(ApiResponse::success(
        ChangeTargetBranchResponse {
            new_target_branch,
            status,
        },
    )))
}

#[axum::debug_handler]
pub async fn rename_branch(
    Extension(task_attempt): Extension<TaskAttempt>,
    remote_ctx: Option<Extension<RemoteTaskAttemptContext>>,
    State(deployment): State<DeploymentImpl>,
    Json(payload): Json<RenameBranchRequest>,
) -> Result<ResponseJson<ApiResponse<RenameBranchResponse>>, ApiError> {
    // Check if this is a remote task attempt that should be proxied
    if let Some((node_url, node_id, shared_task_id)) =
        check_remote_task_attempt_proxy(remote_ctx.as_ref().map(|e| &e.0))?
    {
        tracing::debug!(
            node_id = %node_id,
            shared_task_id = %shared_task_id,
            "Proxying rename_branch to remote node"
        );

        let path = format!("/task-attempts/by-task-id/{}/rename-branch", shared_task_id);
        let response: ApiResponse<RenameBranchResponse> = deployment
            .node_proxy_client()
            .proxy_post(&node_url, &path, &payload, node_id)
            .await?;

        return Ok(ResponseJson(response));
    }

    let new_branch_name = payload.new_branch_name.trim();

    if new_branch_name.is_empty() {
        return Ok(ResponseJson(ApiResponse::error(
            "Branch name cannot be empty",
        )));
    }

    if new_branch_name == task_attempt.branch {
        return Ok(ResponseJson(ApiResponse::success(RenameBranchResponse {
            branch: task_attempt.branch.clone(),
        })));
    }

    if !git2::Branch::name_is_valid(new_branch_name)? {
        return Ok(ResponseJson(ApiResponse::error(
            "Invalid branch name format",
        )));
    }

    let pool = &deployment.db().pool;
    let task = task_attempt
        .parent_task(pool)
        .await?
        .ok_or(ApiError::TaskAttempt(TaskAttemptError::TaskNotFound))?;

    let project = Project::find_by_id(pool, task.project_id)
        .await?
        .ok_or(ApiError::Project(ProjectError::ProjectNotFound))?;

    if deployment
        .git()
        .check_branch_exists(&project.git_repo_path, new_branch_name)?
    {
        return Ok(ResponseJson(ApiResponse::error(
            "A branch with this name already exists",
        )));
    }

    let worktree_path_buf = ensure_worktree_path(&deployment, &task_attempt).await?;
    let worktree_path = worktree_path_buf.as_path();

    if deployment.git().is_rebase_in_progress(worktree_path)? {
        return Ok(ResponseJson(ApiResponse::error(
            "Cannot rename branch while rebase is in progress. Please complete or abort the rebase first.",
        )));
    }

    if let Some(merge) = Merge::find_latest_by_task_attempt_id(pool, task_attempt.id).await?
        && let Merge::Pr(pr_merge) = merge
        && matches!(pr_merge.pr_info.status, MergeStatus::Open)
    {
        return Ok(ResponseJson(ApiResponse::error(
            "Cannot rename branch with an open pull request. Please close the PR first or create a new attempt.",
        )));
    }

    deployment
        .git()
        .rename_local_branch(worktree_path, &task_attempt.branch, new_branch_name)?;

    let old_branch = task_attempt.branch.clone();

    TaskAttempt::update_branch_name(pool, task_attempt.id, new_branch_name).await?;

    let updated_children_count = TaskAttempt::update_target_branch_for_children_of_task(
        pool,
        task_attempt.task_id,
        &old_branch,
        new_branch_name,
    )
    .await?;

    if updated_children_count > 0 {
        tracing::info!(
            "Updated {} child task attempts to target new branch '{}'",
            updated_children_count,
            new_branch_name
        );
    }

    Ok(ResponseJson(ApiResponse::success(RenameBranchResponse {
        branch: new_branch_name.to_string(),
    })))
}

#[axum::debug_handler]
pub async fn rebase_task_attempt(
    Extension(task_attempt): Extension<TaskAttempt>,
    remote_ctx: Option<Extension<RemoteTaskAttemptContext>>,
    State(deployment): State<DeploymentImpl>,
    Json(payload): Json<RebaseTaskAttemptRequest>,
) -> Result<ResponseJson<ApiResponse<(), GitOperationError>>, ApiError> {
    // Check if this is a remote task attempt that should be proxied
    if let Some((node_url, node_id, shared_task_id)) =
        check_remote_task_attempt_proxy(remote_ctx.as_ref().map(|e| &e.0))?
    {
        tracing::debug!(
            node_id = %node_id,
            shared_task_id = %shared_task_id,
            "Proxying rebase_task_attempt to remote node"
        );

        let path = format!("/task-attempts/by-task-id/{}/rebase", shared_task_id);
        let response: ApiResponse<(), GitOperationError> = deployment
            .node_proxy_client()
            .proxy_post(&node_url, &path, &payload, node_id)
            .await?;

        return Ok(ResponseJson(response));
    }

    let old_base_branch = payload
        .old_base_branch
        .unwrap_or(task_attempt.target_branch.clone());
    let new_base_branch = payload
        .new_base_branch
        .unwrap_or(task_attempt.target_branch.clone());

    let pool = &deployment.db().pool;

    let task = task_attempt
        .parent_task(pool)
        .await?
        .ok_or(ApiError::TaskAttempt(TaskAttemptError::TaskNotFound))?;
    let ctx = TaskAttempt::load_context(pool, task_attempt.id, task.id, task.project_id).await?;
    match deployment
        .git()
        .check_branch_exists(&ctx.project.git_repo_path, &new_base_branch)?
    {
        true => {
            TaskAttempt::update_target_branch(
                &deployment.db().pool,
                task_attempt.id,
                &new_base_branch,
            )
            .await?;
        }
        false => {
            return Ok(ResponseJson(ApiResponse::error(
                format!(
                    "Branch '{}' does not exist in the repository",
                    new_base_branch
                )
                .as_str(),
            )));
        }
    }

    let worktree_path_buf = ensure_worktree_path(&deployment, &task_attempt).await?;
    let worktree_path = worktree_path_buf.as_path();

    let result = deployment.git().rebase_branch(
        &ctx.project.git_repo_path,
        worktree_path,
        &new_base_branch,
        &old_base_branch,
        &task_attempt.branch.clone(),
    );
    if let Err(e) = result {
        use services::services::git::GitServiceError;
        return match e {
            GitServiceError::MergeConflicts(msg) => Ok(ResponseJson(ApiResponse::<
                (),
                GitOperationError,
            >::error_with_data(
                GitOperationError::MergeConflicts {
                    message: msg,
                    op: ConflictOp::Rebase,
                },
            ))),
            GitServiceError::RebaseInProgress => Ok(ResponseJson(ApiResponse::<
                (),
                GitOperationError,
            >::error_with_data(
                GitOperationError::RebaseInProgress,
            ))),
            other => Err(ApiError::GitService(other)),
        };
    }

    Ok(ResponseJson(ApiResponse::success(())))
}

#[axum::debug_handler]
pub async fn abort_conflicts_task_attempt(
    Extension(task_attempt): Extension<TaskAttempt>,
    remote_ctx: Option<Extension<RemoteTaskAttemptContext>>,
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<()>>, ApiError> {
    // Check if this is a remote task attempt that should be proxied
    if let Some((node_url, node_id, shared_task_id)) =
        check_remote_task_attempt_proxy(remote_ctx.as_ref().map(|e| &e.0))?
    {
        tracing::debug!(
            node_id = %node_id,
            shared_task_id = %shared_task_id,
            "Proxying abort_conflicts_task_attempt to remote node"
        );

        let path = format!(
            "/task-attempts/by-task-id/{}/conflicts/abort",
            shared_task_id
        );
        let response: ApiResponse<()> = deployment
            .node_proxy_client()
            .proxy_post(&node_url, &path, &(), node_id)
            .await?;

        return Ok(ResponseJson(response));
    }

    // Resolve worktree path for this attempt
    let worktree_path_buf = ensure_worktree_path(&deployment, &task_attempt).await?;
    let worktree_path = worktree_path_buf.as_path();

    deployment.git().abort_conflicts(worktree_path)?;

    Ok(ResponseJson(ApiResponse::success(())))
}

/// Response for get_dirty_files endpoint
#[derive(Debug, Serialize, Deserialize, TS)]
pub struct DirtyFilesResponse {
    pub files: Vec<String>,
}

/// Request for stash_changes endpoint
#[derive(Debug, Deserialize, Serialize, TS)]
pub struct StashChangesRequest {
    pub message: Option<String>,
}

/// Response for stash_changes endpoint
#[derive(Debug, Serialize, Deserialize, TS)]
pub struct StashChangesResponse {
    pub stash_ref: String,
}

/// Get list of dirty (uncommitted) files in the task attempt worktree.
/// This is used by the frontend to display which files would be affected by a stash.
#[axum::debug_handler]
pub async fn get_dirty_files(
    Extension(task_attempt): Extension<TaskAttempt>,
    remote_ctx: Option<Extension<RemoteTaskAttemptContext>>,
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<DirtyFilesResponse>>, ApiError> {
    // Check if this is a remote task attempt that should be proxied
    if let Some((node_url, node_id, shared_task_id)) =
        check_remote_task_attempt_proxy(remote_ctx.as_ref().map(|e| &e.0))?
    {
        tracing::debug!(
            node_id = %node_id,
            shared_task_id = %shared_task_id,
            "Proxying get_dirty_files to remote node"
        );

        let path = format!("/task-attempts/by-task-id/{}/stash/dirty-files", shared_task_id);
        let response: ApiResponse<DirtyFilesResponse> = deployment
            .node_proxy_client()
            .proxy_get(&node_url, &path, node_id)
            .await?;

        return Ok(ResponseJson(response));
    }

    // Resolve worktree path for this attempt
    let worktree_path_buf = ensure_worktree_path(&deployment, &task_attempt).await?;
    let worktree_path = worktree_path_buf.as_path();

    let files = deployment.git().get_dirty_files(worktree_path)?;

    Ok(ResponseJson(ApiResponse::success(DirtyFilesResponse {
        files,
    })))
}

/// Stash uncommitted changes in the task attempt worktree.
/// This allows merge/rebase operations to proceed when there are uncommitted changes.
#[axum::debug_handler]
pub async fn stash_changes(
    Extension(task_attempt): Extension<TaskAttempt>,
    remote_ctx: Option<Extension<RemoteTaskAttemptContext>>,
    State(deployment): State<DeploymentImpl>,
    Json(payload): Json<StashChangesRequest>,
) -> Result<ResponseJson<ApiResponse<StashChangesResponse>>, ApiError> {
    // Check if this is a remote task attempt that should be proxied
    if let Some((node_url, node_id, shared_task_id)) =
        check_remote_task_attempt_proxy(remote_ctx.as_ref().map(|e| &e.0))?
    {
        tracing::debug!(
            node_id = %node_id,
            shared_task_id = %shared_task_id,
            "Proxying stash_changes to remote node"
        );

        let path = format!("/task-attempts/by-task-id/{}/stash", shared_task_id);
        let response: ApiResponse<StashChangesResponse> = deployment
            .node_proxy_client()
            .proxy_post(&node_url, &path, &payload, node_id)
            .await?;

        return Ok(ResponseJson(response));
    }

    // Resolve worktree path for this attempt
    let worktree_path_buf = ensure_worktree_path(&deployment, &task_attempt).await?;
    let worktree_path = worktree_path_buf.as_path();

    let stash_ref = deployment
        .git()
        .stash_changes(worktree_path, payload.message.as_deref())?;

    Ok(ResponseJson(ApiResponse::success(StashChangesResponse {
        stash_ref,
    })))
}

/// Pop the most recent stash, restoring uncommitted changes to the worktree.
#[axum::debug_handler]
pub async fn pop_stash(
    Extension(task_attempt): Extension<TaskAttempt>,
    remote_ctx: Option<Extension<RemoteTaskAttemptContext>>,
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<()>>, ApiError> {
    // Check if this is a remote task attempt that should be proxied
    if let Some((node_url, node_id, shared_task_id)) =
        check_remote_task_attempt_proxy(remote_ctx.as_ref().map(|e| &e.0))?
    {
        tracing::debug!(
            node_id = %node_id,
            shared_task_id = %shared_task_id,
            "Proxying pop_stash to remote node"
        );

        let path = format!("/task-attempts/by-task-id/{}/stash/pop", shared_task_id);
        let response: ApiResponse<()> = deployment
            .node_proxy_client()
            .proxy_post(&node_url, &path, &(), node_id)
            .await?;

        return Ok(ResponseJson(response));
    }

    // Resolve worktree path for this attempt
    let worktree_path_buf = ensure_worktree_path(&deployment, &task_attempt).await?;
    let worktree_path = worktree_path_buf.as_path();

    deployment.git().pop_stash(worktree_path)?;

    Ok(ResponseJson(ApiResponse::success(())))
}

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

pub async fn get_task_attempt_children(
    Extension(task_attempt): Extension<TaskAttempt>,
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<TaskRelationships>>, StatusCode> {
    match Task::find_relationships_for_attempt(&deployment.db().pool, &task_attempt).await {
        Ok(relationships) => Ok(ResponseJson(ApiResponse::success(relationships))),
        Err(e) => {
            tracing::error!(
                "Failed to fetch relationships for task attempt {}: {}",
                task_attempt.id,
                e
            );
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn stop_task_attempt_execution(
    Extension(task_attempt): Extension<TaskAttempt>,
    remote_ctx: Option<Extension<RemoteTaskAttemptContext>>,
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<()>>, ApiError> {
    // Check if this is a remote task attempt that should be proxied
    if let Some((node_url, node_id, shared_task_id)) =
        check_remote_task_attempt_proxy(remote_ctx.as_ref().map(|e| &e.0))?
    {
        tracing::debug!(
            node_id = %node_id,
            shared_task_id = %shared_task_id,
            "Proxying stop_task_attempt_execution to remote node"
        );

        let path = format!("/task-attempts/by-task-id/{}/stop", shared_task_id);
        let response: ApiResponse<()> = deployment
            .node_proxy_client()
            .proxy_post(&node_url, &path, &(), node_id)
            .await?;

        return Ok(ResponseJson(response));
    }

    deployment.container().try_stop(&task_attempt).await;

    Ok(ResponseJson(ApiResponse::success(())))
}

#[derive(Debug, Serialize, Deserialize, TS)]
pub struct AttachPrResponse {
    pub pr_attached: bool,
    pub pr_url: Option<String>,
    pub pr_number: Option<i64>,
    pub pr_status: Option<MergeStatus>,
}

pub async fn attach_existing_pr(
    Extension(task_attempt): Extension<TaskAttempt>,
    remote_ctx: Option<Extension<RemoteTaskAttemptContext>>,
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<AttachPrResponse>>, ApiError> {
    // Check if this is a remote task attempt that should be proxied
    if let Some((node_url, node_id, shared_task_id)) =
        check_remote_task_attempt_proxy(remote_ctx.as_ref().map(|e| &e.0))?
    {
        tracing::debug!(
            node_id = %node_id,
            shared_task_id = %shared_task_id,
            "Proxying attach_existing_pr to remote node"
        );

        let path = format!("/task-attempts/by-task-id/{}/pr/attach", shared_task_id);
        let response: ApiResponse<AttachPrResponse> = deployment
            .node_proxy_client()
            .proxy_post(&node_url, &path, &(), node_id)
            .await?;

        return Ok(ResponseJson(response));
    }

    let pool = &deployment.db().pool;

    // Check if PR already attached
    if let Some(Merge::Pr(pr_merge)) =
        Merge::find_latest_by_task_attempt_id(pool, task_attempt.id).await?
    {
        return Ok(ResponseJson(ApiResponse::success(AttachPrResponse {
            pr_attached: true,
            pr_url: Some(pr_merge.pr_info.url.clone()),
            pr_number: Some(pr_merge.pr_info.number),
            pr_status: Some(pr_merge.pr_info.status.clone()),
        })));
    }

    // Get project and repo info
    let Some(task) = task_attempt.parent_task(pool).await? else {
        return Err(ApiError::TaskAttempt(TaskAttemptError::TaskNotFound));
    };
    let Some(project) = Project::find_by_id(pool, task.project_id).await? else {
        return Err(ApiError::Project(ProjectError::ProjectNotFound));
    };

    let github_service = GitHubService::new()?;
    let repo_info = deployment
        .git()
        .get_github_repo_info(&project.git_repo_path)?;

    // List all PRs for branch (open, closed, and merged)
    let prs = github_service
        .list_all_prs_for_branch(&repo_info, &task_attempt.branch)
        .await?;

    // Take the first PR (prefer open, but also accept merged/closed)
    if let Some(pr_info) = prs.into_iter().next() {
        // Save PR info to database
        let merge = Merge::create_pr(
            pool,
            task_attempt.id,
            &task_attempt.target_branch,
            pr_info.number,
            &pr_info.url,
        )
        .await?;

        // Update status if not open
        if !matches!(pr_info.status, MergeStatus::Open) {
            Merge::update_status(
                pool,
                merge.id,
                pr_info.status.clone(),
                pr_info.merge_commit_sha.clone(),
            )
            .await?;
        }

        // If PR is merged, mark task as done
        if matches!(pr_info.status, MergeStatus::Merged) {
            Task::update_status(pool, task.id, TaskStatus::Done).await?;

            // Try broadcast update to other users in organization
            if let Ok(publisher) = deployment.share_publisher() {
                if let Err(err) = publisher.update_shared_task_by_id(task.id).await {
                    tracing::warn!(
                        ?err,
                        "Failed to propagate shared task update for {}",
                        task.id
                    );
                }
            } else {
                tracing::debug!(
                    "Share publisher unavailable; skipping remote update for {}",
                    task.id
                );
            }
        }

        Ok(ResponseJson(ApiResponse::success(AttachPrResponse {
            pr_attached: true,
            pr_url: Some(pr_info.url),
            pr_number: Some(pr_info.number),
            pr_status: Some(pr_info.status),
        })))
    } else {
        Ok(ResponseJson(ApiResponse::success(AttachPrResponse {
            pr_attached: false,
            pr_url: None,
            pr_number: None,
            pr_status: None,
        })))
    }
}

#[axum::debug_handler]
pub async fn gh_cli_setup_handler(
    Extension(task_attempt): Extension<TaskAttempt>,
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<ExecutionProcess, GhCliSetupError>>, ApiError> {
    match gh_cli_setup::run_gh_cli_setup(&deployment, &task_attempt).await {
        Ok(execution_process) => Ok(ResponseJson(ApiResponse::success(execution_process))),
        Err(ApiError::Executor(ExecutorError::ExecutableNotFound { program }))
            if program == "brew" =>
        {
            Ok(ResponseJson(ApiResponse::error_with_data(
                GhCliSetupError::BrewMissing,
            )))
        }
        Err(ApiError::Executor(ExecutorError::SetupHelperNotSupported)) => Ok(ResponseJson(
            ApiResponse::error_with_data(GhCliSetupError::SetupHelperNotSupported),
        )),
        Err(ApiError::Executor(err)) => Ok(ResponseJson(ApiResponse::error_with_data(
            GhCliSetupError::Other {
                message: err.to_string(),
            },
        ))),
        Err(err) => Err(err),
    }
}

// ============================================================================
// File Browser Endpoints
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct ListFilesQuery {
    /// Relative path within the worktree (optional, defaults to root)
    path: Option<String>,
}

/// List files and directories within a task attempt's worktree
pub async fn list_worktree_files(
    Extension(task_attempt): Extension<TaskAttempt>,
    remote_ctx: Option<Extension<RemoteTaskAttemptContext>>,
    State(deployment): State<DeploymentImpl>,
    Query(query): Query<ListFilesQuery>,
) -> Result<ResponseJson<ApiResponse<DirectoryListResponse>>, ApiError> {
    // Check if this is a remote task attempt that should be proxied
    if let Some((node_url, node_id, shared_task_id)) =
        check_remote_task_attempt_proxy(remote_ctx.as_ref().map(|e| &e.0))?
    {
        tracing::debug!(
            node_id = %node_id,
            shared_task_id = %shared_task_id,
            "Proxying list_worktree_files to remote node"
        );

        let path = match &query.path {
            Some(p) => format!(
                "/task-attempts/by-task-id/{}/files?path={}",
                shared_task_id,
                urlencoding::encode(p)
            ),
            None => format!("/task-attempts/by-task-id/{}/files", shared_task_id),
        };
        let response: ApiResponse<DirectoryListResponse> = deployment
            .node_proxy_client()
            .proxy_get(&node_url, &path, node_id)
            .await?;

        return Ok(ResponseJson(response));
    }

    let worktree_path = ensure_worktree_path(&deployment, &task_attempt).await?;

    match deployment
        .filesystem()
        .list_directory_within(&worktree_path, query.path.as_deref())
        .await
    {
        Ok(response) => Ok(ResponseJson(ApiResponse::success(response))),
        Err(FilesystemError::DirectoryDoesNotExist) => {
            Ok(ResponseJson(ApiResponse::error("Directory does not exist")))
        }
        Err(FilesystemError::PathIsNotDirectory) => {
            Ok(ResponseJson(ApiResponse::error("Path is not a directory")))
        }
        Err(FilesystemError::PathTraversalNotAllowed) => Ok(ResponseJson(ApiResponse::error(
            "Path traversal not allowed",
        ))),
        Err(FilesystemError::Io(e)) => {
            tracing::error!("Failed to list worktree directory: {}", e);
            Ok(ResponseJson(ApiResponse::error(&format!(
                "Failed to list directory: {}",
                e
            ))))
        }
        Err(e) => {
            tracing::error!("Unexpected error listing worktree: {}", e);
            Ok(ResponseJson(ApiResponse::error(&e.to_string())))
        }
    }
}

/// Read file content from a task attempt's worktree
pub async fn read_worktree_file(
    Extension(task_attempt): Extension<TaskAttempt>,
    remote_ctx: Option<Extension<RemoteTaskAttemptContext>>,
    State(deployment): State<DeploymentImpl>,
    Path((_, file_path)): Path<(String, String)>,
) -> Result<ResponseJson<ApiResponse<FileContentResponse>>, ApiError> {
    // Check if this is a remote task attempt that should be proxied
    if let Some((node_url, node_id, shared_task_id)) =
        check_remote_task_attempt_proxy(remote_ctx.as_ref().map(|e| &e.0))?
    {
        tracing::debug!(
            node_id = %node_id,
            shared_task_id = %shared_task_id,
            file_path = %file_path,
            "Proxying read_worktree_file to remote node"
        );

        let path = format!(
            "/task-attempts/by-task-id/{}/files/{}",
            shared_task_id, file_path
        );
        let response: ApiResponse<FileContentResponse> = deployment
            .node_proxy_client()
            .proxy_get(&node_url, &path, node_id)
            .await?;

        return Ok(ResponseJson(response));
    }

    let worktree_path = ensure_worktree_path(&deployment, &task_attempt).await?;

    match deployment
        .filesystem()
        .read_file_within(&worktree_path, &file_path, None)
        .await
    {
        Ok(response) => Ok(ResponseJson(ApiResponse::success(response))),
        Err(FilesystemError::FileDoesNotExist) => {
            Ok(ResponseJson(ApiResponse::error("File does not exist")))
        }
        Err(FilesystemError::PathIsNotFile) => {
            Ok(ResponseJson(ApiResponse::error("Path is not a file")))
        }
        Err(FilesystemError::PathTraversalNotAllowed) => Ok(ResponseJson(ApiResponse::error(
            "Path traversal not allowed",
        ))),
        Err(FilesystemError::FileIsBinary) => Ok(ResponseJson(ApiResponse::error(
            "Cannot display binary file",
        ))),
        Err(FilesystemError::FileTooLarge {
            max_bytes,
            actual_bytes,
        }) => Ok(ResponseJson(ApiResponse::error(&format!(
            "File too large ({} bytes, max {} bytes)",
            actual_bytes, max_bytes
        )))),
        Err(FilesystemError::Io(e)) => {
            tracing::error!("Failed to read worktree file: {}", e);
            Ok(ResponseJson(ApiResponse::error(&format!(
                "Failed to read file: {}",
                e
            ))))
        }
        Err(e) => {
            tracing::error!("Unexpected error reading file: {}", e);
            Ok(ResponseJson(ApiResponse::error(&e.to_string())))
        }
    }
}

/// Response for getting the worktree path
#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct WorktreePathResponse {
    /// Absolute path to the worktree directory
    pub path: String,
}

/// Get the worktree path for a task attempt
///
/// GET /api/task-attempts/{id}/worktree-path
///
/// Returns the absolute path to the worktree directory for the given task attempt.
/// This is useful for opening a terminal in the worktree directory.
pub async fn get_worktree_path(
    Extension(task_attempt): Extension<TaskAttempt>,
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<WorktreePathResponse>>, ApiError> {
    let worktree_path = ensure_worktree_path(&deployment, &task_attempt).await?;
    let path_string = worktree_path
        .to_str()
        .ok_or_else(|| ApiError::BadRequest("Invalid worktree path".to_string()))?
        .to_string();

    Ok(ResponseJson(ApiResponse::success(WorktreePathResponse {
        path: path_string,
    })))
}

pub fn router(deployment: &DeploymentImpl) -> Router<DeploymentImpl> {
    let task_attempt_id_router = Router::new()
        .route("/", get(get_task_attempt))
        .route("/follow-up", post(follow_up))
        .route("/run-agent-setup", post(run_agent_setup))
        .route("/gh-cli-setup", post(gh_cli_setup_handler))
        .route(
            "/draft",
            get(drafts::get_draft)
                .put(drafts::save_draft)
                .delete(drafts::delete_draft),
        )
        .route("/draft/queue", post(drafts::set_draft_queue))
        .route("/commit-info", get(get_commit_info))
        .route("/commit-compare", get(compare_commit_to_head))
        .route("/start-dev-server", post(start_dev_server))
        .route("/branch-status", get(get_task_attempt_branch_status))
        .route("/diff/ws", get(stream_task_attempt_diff_ws))
        .route("/merge", post(merge_task_attempt))
        .route("/push", post(push_task_attempt_branch))
        .route("/push/force", post(force_push_task_attempt_branch))
        .route("/rebase", post(rebase_task_attempt))
        .route("/conflicts/abort", post(abort_conflicts_task_attempt))
        // Stash endpoints for handling uncommitted changes
        .route("/stash/dirty-files", get(get_dirty_files))
        .route("/stash", post(stash_changes))
        .route("/stash/pop", post(pop_stash))
        .route("/pr", post(create_github_pr))
        .route("/pr/attach", post(attach_existing_pr))
        .route("/open-editor", post(open_task_attempt_in_editor))
        .route("/children", get(get_task_attempt_children))
        .route("/stop", post(stop_task_attempt_execution))
        .route("/change-target-branch", post(change_target_branch))
        .route("/rename-branch", post(rename_branch))
        // Worktree path endpoint (for terminal sessions)
        .route("/worktree-path", get(get_worktree_path))
        // File browser endpoints (directory listing only - wildcard route is separate)
        .route("/files", get(list_worktree_files))
        .layer(from_fn_with_state(
            deployment.clone(),
            load_task_attempt_middleware,
        ));

    // Wildcard file path route needs to be separate (not nested) to avoid
    // path parameter count mismatch in the middleware. Uses the wildcard variant
    // that extracts both path params but only uses the id.
    let task_attempt_files_router = Router::new()
        .route("/{id}/files/{*file_path}", get(read_worktree_file))
        .layer(from_fn_with_state(
            deployment.clone(),
            load_task_attempt_middleware_with_wildcard,
        ));

    // Routes for accessing task attempts by shared_task_id (used for node-to-node proxying).
    // These routes allow a proxying node to request data using the Hive shared task ID.
    // The middleware finds the task by shared_task_id and loads its most recent attempt.
    let by_task_id_router = Router::new()
        .route("/follow-up", post(follow_up))
        .route("/stop", post(stop_task_attempt_execution))
        .route("/branch-status", get(get_task_attempt_branch_status))
        .route("/push", post(push_task_attempt_branch))
        .route("/push/force", post(force_push_task_attempt_branch))
        .route("/merge", post(merge_task_attempt))
        .route("/rebase", post(rebase_task_attempt))
        .route("/conflicts/abort", post(abort_conflicts_task_attempt))
        // Stash endpoints for handling uncommitted changes
        .route("/stash/dirty-files", get(get_dirty_files))
        .route("/stash", post(stash_changes))
        .route("/stash/pop", post(pop_stash))
        .route("/change-target-branch", post(change_target_branch))
        .route("/rename-branch", post(rename_branch))
        .route("/pr", post(create_github_pr))
        .route("/pr/attach", post(attach_existing_pr))
        .route(
            "/draft",
            get(drafts::get_draft)
                .put(drafts::save_draft)
                .delete(drafts::delete_draft),
        )
        .route("/draft/queue", post(drafts::set_draft_queue))
        .route("/files", get(list_worktree_files))
        .route("/diff/ws", get(stream_task_attempt_diff_ws))
        .layer(from_fn_with_state(
            deployment.clone(),
            load_task_attempt_by_task_id_middleware,
        ));

    // Wildcard file path route for by-task-id (file content browsing)
    let by_task_id_files_router = Router::new()
        .route(
            "/by-task-id/{task_id}/files/{*file_path}",
            get(read_worktree_file),
        )
        .layer(from_fn_with_state(
            deployment.clone(),
            load_task_attempt_by_task_id_middleware_with_wildcard,
        ));

    // Route for creating task attempts via shared_task_id (cross-node proxying).
    // Uses different middleware that only loads Task (not TaskAttempt).
    let by_task_id_create_router = Router::new()
        .route(
            "/by-task-id/{task_id}/create",
            post(create_task_attempt_by_task_id),
        )
        .layer(from_fn_with_state(
            deployment.clone(),
            load_task_by_task_id_middleware,
        ));

    let task_attempts_router = Router::new()
        .route("/", get(get_task_attempts).post(create_task_attempt))
        .nest("/{id}", task_attempt_id_router)
        .merge(task_attempt_files_router)
        .nest("/by-task-id/{task_id}", by_task_id_router)
        .merge(by_task_id_files_router)
        .merge(by_task_id_create_router);

    Router::new().nest("/task-attempts", task_attempts_router)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::middleware::RemoteTaskAttemptContext;

    #[test]
    fn test_check_remote_proxy_returns_none_when_no_context() {
        let result = check_remote_task_attempt_proxy(None);
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[test]
    fn test_check_remote_proxy_returns_error_when_node_offline() {
        let ctx = RemoteTaskAttemptContext {
            node_id: Uuid::new_v4(),
            node_url: Some("http://node:3000".to_string()),
            node_status: Some("offline".to_string()),
            task_id: Uuid::new_v4(),
        };
        let result = check_remote_task_attempt_proxy(Some(&ctx));
        assert!(result.is_err());
        match result {
            Err(ApiError::BadGateway(msg)) => {
                assert!(msg.contains("offline"));
            }
            _ => panic!("Expected BadGateway error"),
        }
    }

    #[test]
    fn test_check_remote_proxy_returns_error_when_no_node_url() {
        let ctx = RemoteTaskAttemptContext {
            node_id: Uuid::new_v4(),
            node_url: None,
            node_status: Some("online".to_string()),
            task_id: Uuid::new_v4(),
        };
        let result = check_remote_task_attempt_proxy(Some(&ctx));
        assert!(result.is_err());
        match result {
            Err(ApiError::BadGateway(msg)) => {
                assert!(msg.contains("no public URL"));
            }
            _ => panic!("Expected BadGateway error"),
        }
    }

    #[test]
    fn test_check_remote_proxy_returns_info_when_node_online() {
        let node_id = Uuid::new_v4();
        let task_id = Uuid::new_v4();
        let ctx = RemoteTaskAttemptContext {
            node_id,
            node_url: Some("http://node:3000".to_string()),
            node_status: Some("online".to_string()),
            task_id,
        };
        let result = check_remote_task_attempt_proxy(Some(&ctx));
        assert!(result.is_ok());
        let proxy_info = result.unwrap();
        assert!(proxy_info.is_some());
        let (url, returned_node_id, returned_task_id) = proxy_info.unwrap();
        assert_eq!(url, "http://node:3000");
        assert_eq!(returned_node_id, node_id);
        assert_eq!(returned_task_id, task_id);
    }

    #[test]
    fn test_create_task_attempt_body_with_use_parent_worktree() {
        let body: CreateTaskAttemptBody = serde_json::from_str(
            r#"{
            "task_id": "550e8400-e29b-41d4-a716-446655440000",
            "executor_profile_id": { "executor": "CLAUDE_CODE", "variant": null },
            "base_branch": "main",
            "use_parent_worktree": true
        }"#,
        )
        .unwrap();
        assert!(body.use_parent_worktree.unwrap_or(false));
    }

    #[test]
    fn test_create_task_attempt_body_backwards_compatible() {
        // Old requests without use_parent_worktree should still work
        let body: CreateTaskAttemptBody = serde_json::from_str(
            r#"{
            "task_id": "550e8400-e29b-41d4-a716-446655440000",
            "executor_profile_id": { "executor": "CLAUDE_CODE", "variant": null },
            "base_branch": "main"
        }"#,
        )
        .unwrap();
        assert!(body.use_parent_worktree.is_none());
    }

    #[test]
    fn test_create_task_attempt_body_with_use_parent_worktree_false() {
        let body: CreateTaskAttemptBody = serde_json::from_str(
            r#"{
            "task_id": "550e8400-e29b-41d4-a716-446655440000",
            "executor_profile_id": { "executor": "CLAUDE_CODE", "variant": null },
            "base_branch": "main",
            "use_parent_worktree": false
        }"#,
        )
        .unwrap();
        assert_eq!(body.use_parent_worktree, Some(false));
    }

    #[test]
    fn test_create_task_attempt_by_task_id_body_with_use_parent_worktree() {
        let body: CreateTaskAttemptByTaskIdBody = serde_json::from_str(
            r#"{
            "executor_profile_id": { "executor": "CLAUDE_CODE", "variant": null },
            "base_branch": "main",
            "use_parent_worktree": true
        }"#,
        )
        .unwrap();
        assert!(body.use_parent_worktree.unwrap_or(false));
    }

    #[test]
    fn test_create_task_attempt_by_task_id_body_backwards_compatible() {
        // Old requests without use_parent_worktree should still work
        let body: CreateTaskAttemptByTaskIdBody = serde_json::from_str(
            r#"{
            "executor_profile_id": { "executor": "CLAUDE_CODE", "variant": null },
            "base_branch": "main"
        }"#,
        )
        .unwrap();
        assert!(body.use_parent_worktree.is_none());
    }
}
