//! GitHub-related handlers: PR creation, PR attachment, gh CLI setup.

use axum::{Extension, Json, extract::State, response::Json as ResponseJson};
use db::models::{
    execution_process::ExecutionProcess,
    merge::{Merge, MergeStatus},
    project::{Project, ProjectError},
    task::{Task, TaskStatus},
    task_attempt::{TaskAttempt, TaskAttemptError},
};
use deployment::Deployment;
use git2::BranchType;
use services::services::{
    git::{GitCliError, GitServiceError},
    github::{CreatePrRequest, GitHubService, GitHubServiceError},
};
use utils::response::ApiResponse;

use crate::routes::task_attempts::gh_cli_setup::{self, GhCliSetupError};
use crate::routes::task_attempts::types::{AttachPrResponse, CreateGitHubPrRequest, CreatePrError};
use crate::routes::task_attempts::util::ensure_worktree_path;
use crate::{
    DeploymentImpl, error::ApiError, middleware::RemoteTaskAttemptContext,
    proxy::check_remote_task_attempt_proxy,
};
use executors::executors::ExecutorError;

pub async fn create_github_pr(
    Extension(task_attempt): Extension<TaskAttempt>,
    remote_ctx: Option<Extension<RemoteTaskAttemptContext>>,
    State(deployment): State<DeploymentImpl>,
    Json(request): Json<CreateGitHubPrRequest>,
) -> Result<ResponseJson<ApiResponse<String, CreatePrError>>, ApiError> {
    // Check if this is a remote task attempt that should be proxied
    if let Some(proxy_info) = check_remote_task_attempt_proxy(remote_ctx.as_ref().map(|e| &e.0))? {
        tracing::debug!(
            node_id = %proxy_info.node_id,
            shared_task_id = %proxy_info.target_id,
            "Proxying create_github_pr to remote node"
        );

        let path = format!("/task-attempts/by-task-id/{}/pr", proxy_info.target_id);
        let response: ApiResponse<String, CreatePrError> = deployment
            .node_proxy_client()
            .proxy_post(&proxy_info.node_url, &path, &request, proxy_info.node_id)
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

pub async fn attach_existing_pr(
    Extension(task_attempt): Extension<TaskAttempt>,
    remote_ctx: Option<Extension<RemoteTaskAttemptContext>>,
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<AttachPrResponse>>, ApiError> {
    // Check if this is a remote task attempt that should be proxied
    if let Some(proxy_info) = check_remote_task_attempt_proxy(remote_ctx.as_ref().map(|e| &e.0))? {
        tracing::debug!(
            node_id = %proxy_info.node_id,
            shared_task_id = %proxy_info.target_id,
            "Proxying attach_existing_pr to remote node"
        );

        let path = format!(
            "/task-attempts/by-task-id/{}/pr/attach",
            proxy_info.target_id
        );
        let response: ApiResponse<AttachPrResponse> = deployment
            .node_proxy_client()
            .proxy_post(&proxy_info.node_url, &path, &(), proxy_info.node_id)
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
