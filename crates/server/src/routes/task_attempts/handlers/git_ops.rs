//! Git operations handlers: merge, rebase, push, stash, branch operations.

use axum::{
    Extension, Json,
    extract::State,
    response::Json as ResponseJson,
};
use db::models::{
    execution_process::{ExecutionProcess, ExecutionProcessStatus},
    merge::Merge,
    merge::{MergeStatus, PrMerge},
    project::{Project, ProjectError},
    task::{Task, TaskStatus},
    task_attempt::{TaskAttempt, TaskAttemptError},
};
use git2::BranchType;
use deployment::Deployment;
use services::services::{
    container::ContainerService,
    git::{ConflictOp, GitCliError, GitServiceError},
    github::GitHubService,
};
use utils::response::ApiResponse;

use crate::{DeploymentImpl, error::ApiError, middleware::RemoteTaskAttemptContext, proxy::check_remote_task_attempt_proxy};
use crate::routes::task_attempts::types::{
    BranchStatus, ChangeTargetBranchRequest, ChangeTargetBranchResponse,
    DirtyFilesResponse, GitOperationError, PushError, RebaseTaskAttemptRequest,
    RenameBranchRequest, RenameBranchResponse, StashChangesRequest, StashChangesResponse,
};
use crate::routes::task_attempts::util::ensure_worktree_path;

// ============================================================================
// Merge
// ============================================================================

#[axum::debug_handler]
pub async fn merge_task_attempt(
    Extension(task_attempt): Extension<TaskAttempt>,
    remote_ctx: Option<Extension<RemoteTaskAttemptContext>>,
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<()>>, ApiError> {
    // Check if this is a remote task attempt that should be proxied
    if let Some(proxy_info) =
        check_remote_task_attempt_proxy(remote_ctx.as_ref().map(|e| &e.0))?
    {
        tracing::debug!(
            node_id = %proxy_info.node_id,
            shared_task_id = %proxy_info.target_id,
            "Proxying merge_task_attempt to remote node"
        );

        let path = format!("/task-attempts/by-task-id/{}/merge", proxy_info.target_id);
        let response: ApiResponse<()> = deployment
            .node_proxy_client()
            .proxy_post(&proxy_info.node_url, &path, &(), proxy_info.node_id)
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

// ============================================================================
// Push
// ============================================================================

pub async fn push_task_attempt_branch(
    Extension(task_attempt): Extension<TaskAttempt>,
    remote_ctx: Option<Extension<RemoteTaskAttemptContext>>,
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<(), PushError>>, ApiError> {
    // Check if this is a remote task attempt that should be proxied
    if let Some(proxy_info) =
        check_remote_task_attempt_proxy(remote_ctx.as_ref().map(|e| &e.0))?
    {
        tracing::debug!(
            node_id = %proxy_info.node_id,
            shared_task_id = %proxy_info.target_id,
            "Proxying push_task_attempt_branch to remote node"
        );

        let path = format!("/task-attempts/by-task-id/{}/push", proxy_info.target_id);
        let response: ApiResponse<(), PushError> = deployment
            .node_proxy_client()
            .proxy_post(&proxy_info.node_url, &path, &(), proxy_info.node_id)
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
    if let Some(proxy_info) =
        check_remote_task_attempt_proxy(remote_ctx.as_ref().map(|e| &e.0))?
    {
        tracing::debug!(
            node_id = %proxy_info.node_id,
            shared_task_id = %proxy_info.target_id,
            "Proxying force_push_task_attempt_branch to remote node"
        );

        let path = format!("/task-attempts/by-task-id/{}/push/force", proxy_info.target_id);
        let response: ApiResponse<(), PushError> = deployment
            .node_proxy_client()
            .proxy_post(&proxy_info.node_url, &path, &(), proxy_info.node_id)
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

// ============================================================================
// Rebase and Conflict Resolution
// ============================================================================

#[axum::debug_handler]
pub async fn rebase_task_attempt(
    Extension(task_attempt): Extension<TaskAttempt>,
    remote_ctx: Option<Extension<RemoteTaskAttemptContext>>,
    State(deployment): State<DeploymentImpl>,
    Json(payload): Json<RebaseTaskAttemptRequest>,
) -> Result<ResponseJson<ApiResponse<(), GitOperationError>>, ApiError> {
    // Check if this is a remote task attempt that should be proxied
    if let Some(proxy_info) =
        check_remote_task_attempt_proxy(remote_ctx.as_ref().map(|e| &e.0))?
    {
        tracing::debug!(
            node_id = %proxy_info.node_id,
            shared_task_id = %proxy_info.target_id,
            "Proxying rebase_task_attempt to remote node"
        );

        let path = format!("/task-attempts/by-task-id/{}/rebase", proxy_info.target_id);
        let response: ApiResponse<(), GitOperationError> = deployment
            .node_proxy_client()
            .proxy_post(&proxy_info.node_url, &path, &payload, proxy_info.node_id)
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
    if let Some(proxy_info) =
        check_remote_task_attempt_proxy(remote_ctx.as_ref().map(|e| &e.0))?
    {
        tracing::debug!(
            node_id = %proxy_info.node_id,
            shared_task_id = %proxy_info.target_id,
            "Proxying abort_conflicts_task_attempt to remote node"
        );

        let path = format!(
            "/task-attempts/by-task-id/{}/conflicts/abort",
            proxy_info.target_id
        );
        let response: ApiResponse<()> = deployment
            .node_proxy_client()
            .proxy_post(&proxy_info.node_url, &path, &(), proxy_info.node_id)
            .await?;

        return Ok(ResponseJson(response));
    }

    // Resolve worktree path for this attempt
    let worktree_path_buf = ensure_worktree_path(&deployment, &task_attempt).await?;
    let worktree_path = worktree_path_buf.as_path();

    deployment.git().abort_conflicts(worktree_path)?;

    Ok(ResponseJson(ApiResponse::success(())))
}

// ============================================================================
// Stash Operations
// ============================================================================

/// Get list of dirty (uncommitted) files in the task attempt worktree.
/// This is used by the frontend to display which files would be affected by a stash.
#[axum::debug_handler]
pub async fn get_dirty_files(
    Extension(task_attempt): Extension<TaskAttempt>,
    remote_ctx: Option<Extension<RemoteTaskAttemptContext>>,
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<DirtyFilesResponse>>, ApiError> {
    // Check if this is a remote task attempt that should be proxied
    if let Some(proxy_info) =
        check_remote_task_attempt_proxy(remote_ctx.as_ref().map(|e| &e.0))?
    {
        tracing::debug!(
            node_id = %proxy_info.node_id,
            shared_task_id = %proxy_info.target_id,
            "Proxying get_dirty_files to remote node"
        );

        let path = format!(
            "/task-attempts/by-task-id/{}/stash/dirty-files",
            proxy_info.target_id
        );
        let response: ApiResponse<DirtyFilesResponse> = deployment
            .node_proxy_client()
            .proxy_get(&proxy_info.node_url, &path, proxy_info.node_id)
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
    if let Some(proxy_info) =
        check_remote_task_attempt_proxy(remote_ctx.as_ref().map(|e| &e.0))?
    {
        tracing::debug!(
            node_id = %proxy_info.node_id,
            shared_task_id = %proxy_info.target_id,
            "Proxying stash_changes to remote node"
        );

        let path = format!("/task-attempts/by-task-id/{}/stash", proxy_info.target_id);
        let response: ApiResponse<StashChangesResponse> = deployment
            .node_proxy_client()
            .proxy_post(&proxy_info.node_url, &path, &payload, proxy_info.node_id)
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
    if let Some(proxy_info) =
        check_remote_task_attempt_proxy(remote_ctx.as_ref().map(|e| &e.0))?
    {
        tracing::debug!(
            node_id = %proxy_info.node_id,
            shared_task_id = %proxy_info.target_id,
            "Proxying pop_stash to remote node"
        );

        let path = format!("/task-attempts/by-task-id/{}/stash/pop", proxy_info.target_id);
        let response: ApiResponse<()> = deployment
            .node_proxy_client()
            .proxy_post(&proxy_info.node_url, &path, &(), proxy_info.node_id)
            .await?;

        return Ok(ResponseJson(response));
    }

    // Resolve worktree path for this attempt
    let worktree_path_buf = ensure_worktree_path(&deployment, &task_attempt).await?;
    let worktree_path = worktree_path_buf.as_path();

    deployment.git().pop_stash(worktree_path)?;

    Ok(ResponseJson(ApiResponse::success(())))
}

// ============================================================================
// Branch Operations
// ============================================================================

pub async fn get_task_attempt_branch_status(
    Extension(task_attempt): Extension<TaskAttempt>,
    remote_ctx: Option<Extension<RemoteTaskAttemptContext>>,
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<BranchStatus>>, ApiError> {
    // Check if this is a remote task attempt that should be proxied
    if let Some(proxy_info) =
        check_remote_task_attempt_proxy(remote_ctx.as_ref().map(|e| &e.0))?
    {
        tracing::debug!(
            node_id = %proxy_info.node_id,
            shared_task_id = %proxy_info.target_id,
            "Proxying get_task_attempt_branch_status to remote node"
        );

        let path = format!("/task-attempts/by-task-id/{}/branch-status", proxy_info.target_id);
        let response: ApiResponse<BranchStatus> = deployment
            .node_proxy_client()
            .proxy_get(&proxy_info.node_url, &path, proxy_info.node_id)
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
        pr_info: db::models::merge::PullRequestInfo {
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

#[axum::debug_handler]
pub async fn change_target_branch(
    Extension(task_attempt): Extension<TaskAttempt>,
    remote_ctx: Option<Extension<RemoteTaskAttemptContext>>,
    State(deployment): State<DeploymentImpl>,
    Json(payload): Json<ChangeTargetBranchRequest>,
) -> Result<ResponseJson<ApiResponse<ChangeTargetBranchResponse>>, ApiError> {
    // Check if this is a remote task attempt that should be proxied
    if let Some(proxy_info) =
        check_remote_task_attempt_proxy(remote_ctx.as_ref().map(|e| &e.0))?
    {
        tracing::debug!(
            node_id = %proxy_info.node_id,
            shared_task_id = %proxy_info.target_id,
            "Proxying change_target_branch to remote node"
        );

        let path = format!(
            "/task-attempts/by-task-id/{}/change-target-branch",
            proxy_info.target_id
        );
        let response: ApiResponse<ChangeTargetBranchResponse> = deployment
            .node_proxy_client()
            .proxy_post(&proxy_info.node_url, &path, &payload, proxy_info.node_id)
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
    if let Some(proxy_info) =
        check_remote_task_attempt_proxy(remote_ctx.as_ref().map(|e| &e.0))?
    {
        tracing::debug!(
            node_id = %proxy_info.node_id,
            shared_task_id = %proxy_info.target_id,
            "Proxying rename_branch to remote node"
        );

        let path = format!("/task-attempts/by-task-id/{}/rename-branch", proxy_info.target_id);
        let response: ApiResponse<RenameBranchResponse> = deployment
            .node_proxy_client()
            .proxy_post(&proxy_info.node_url, &path, &payload, proxy_info.node_id)
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
