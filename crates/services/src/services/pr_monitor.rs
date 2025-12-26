use std::time::Duration;

use db::{
    DBService,
    models::{
        merge::{Merge, MergeStatus, PrMerge},
        task::{Task, TaskStatus},
        task_attempt::{TaskAttempt, TaskAttemptError},
    },
};
use sqlx::error::Error as SqlxError;
use thiserror::Error;
use tokio::time::interval;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::services::{
    github::{GitHubRepoInfo, GitHubService, GitHubServiceError},
    share::SharePublisher,
};

#[derive(Debug, Error)]
enum PrMonitorError {
    #[error(transparent)]
    GitHubServiceError(#[from] GitHubServiceError),
    #[error(transparent)]
    TaskAttemptError(#[from] TaskAttemptError),
    #[error(transparent)]
    Sqlx(#[from] SqlxError),
}

/// Service to monitor GitHub PRs and update task status when they are merged
pub struct PrMonitorService {
    db: DBService,
    poll_interval: Duration,
    publisher: Option<SharePublisher>,
}

impl PrMonitorService {
    pub async fn spawn(
        db: DBService,
        publisher: Option<SharePublisher>,
    ) -> tokio::task::JoinHandle<()> {
        let service = Self {
            db,
            poll_interval: Duration::from_secs(60), // Check every minute
            publisher,
        };
        tokio::spawn(async move {
            service.start().await;
        })
    }

    async fn start(&self) {
        info!(
            "Starting PR monitoring service with interval {:?}",
            self.poll_interval
        );

        let mut interval = interval(self.poll_interval);

        loop {
            interval.tick().await;

            // First, discover any new PRs that agents created but we haven't tracked
            if let Err(e) = self.discover_new_prs().await {
                error!("Error discovering new PRs: {}", e);
            }

            // Then, check status of all tracked open PRs
            if let Err(e) = self.check_all_open_prs().await {
                error!("Error checking open PRs: {}", e);
            }
        }
    }

    /// Discover PRs that were created by agents but not yet tracked in the database.
    /// This scans active task attempts (tasks not in done/cancelled state) that don't
    /// have a PR merge record, queries GitHub to see if a PR exists for their branch,
    /// and creates merge records for any discovered PRs.
    async fn discover_new_prs(&self) -> Result<(), PrMonitorError> {
        // Get attempts that might have untracked PRs
        let attempts_without_pr = TaskAttempt::find_active_without_pr(&self.db.pool).await?;

        if attempts_without_pr.is_empty() {
            debug!("No attempts to check for untracked PRs");
            return Ok(());
        }

        debug!(
            "Checking {} attempts for untracked PRs",
            attempts_without_pr.len()
        );

        for (attempt_id, branch, github_owner, github_repo) in attempts_without_pr {
            if let Err(e) = self
                .discover_pr_for_attempt(attempt_id, &branch, &github_owner, &github_repo)
                .await
            {
                // Log but continue - one failure shouldn't stop other discoveries
                warn!(
                    attempt_id = %attempt_id,
                    branch = %branch,
                    "Failed to discover PR: {}",
                    e
                );
            }
        }

        Ok(())
    }

    /// Check if a PR exists for a specific attempt's branch and create a merge record if found.
    async fn discover_pr_for_attempt(
        &self,
        attempt_id: Uuid,
        branch: &str,
        github_owner: &str,
        github_repo: &str,
    ) -> Result<(), PrMonitorError> {
        let github_service = GitHubService::new()?;
        let repo_info = GitHubRepoInfo {
            owner: github_owner.to_string(),
            repo_name: github_repo.to_string(),
        };

        // Query GitHub for PRs on this branch
        let prs = github_service
            .list_all_prs_for_branch(&repo_info, branch)
            .await?;

        if prs.is_empty() {
            debug!(
                attempt_id = %attempt_id,
                branch = %branch,
                "No PR found for branch"
            );
            return Ok(());
        }

        // Use the most recent PR (first in the list since they're ordered by creation date desc)
        let pr = &prs[0];

        info!(
            attempt_id = %attempt_id,
            branch = %branch,
            pr_number = pr.number,
            "Discovered untracked PR, creating merge record"
        );

        // Need to get the target branch. For discovered PRs, we'll fetch it from the attempt.
        let attempt = TaskAttempt::find_by_id(&self.db.pool, attempt_id)
            .await?
            .ok_or_else(|| PrMonitorError::Sqlx(SqlxError::RowNotFound))?;

        // Create a merge record for this PR
        Merge::create_pr(
            &self.db.pool,
            attempt_id,
            &attempt.target_branch,
            pr.number,
            &pr.url,
        )
        .await?;

        // If the PR is already merged, update the status immediately
        if matches!(pr.status, MergeStatus::Merged) {
            // Get the merge record we just created to update it
            if let Some(Merge::Pr(pr_merge)) =
                Merge::find_latest_by_task_attempt_id(&self.db.pool, attempt_id).await?
            {
                Merge::update_status(
                    &self.db.pool,
                    pr_merge.id,
                    pr.status.clone(),
                    pr.merge_commit_sha.clone(),
                )
                .await?;

                // Also update the task to done if it was merged
                info!(
                    attempt_id = %attempt_id,
                    pr_number = pr.number,
                    "Discovered PR was already merged, updating task to done"
                );
                Task::update_status(&self.db.pool, attempt.task_id, TaskStatus::Done).await?;

                if let Some(publisher) = &self.publisher
                    && let Err(err) = publisher.update_shared_task_by_id(attempt.task_id).await
                {
                    warn!(
                        ?err,
                        "Failed to propagate shared task update for {}", attempt.task_id
                    );
                }
            }
        }

        Ok(())
    }

    /// Check all open PRs for updates with the provided GitHub token
    async fn check_all_open_prs(&self) -> Result<(), PrMonitorError> {
        let open_prs = Merge::get_open_prs(&self.db.pool).await?;

        if open_prs.is_empty() {
            debug!("No open PRs to check");
            return Ok(());
        }

        info!("Checking {} open PRs", open_prs.len());

        for pr_merge in open_prs {
            if let Err(e) = self.check_pr_status(&pr_merge).await {
                error!(
                    "Error checking PR #{} for attempt {}: {}",
                    pr_merge.pr_info.number, pr_merge.task_attempt_id, e
                );
            }
        }
        Ok(())
    }

    /// Check the status of a specific PR
    async fn check_pr_status(&self, pr_merge: &PrMerge) -> Result<(), PrMonitorError> {
        // GitHubService now uses gh CLI, no token needed
        let github_service = GitHubService::new()?;
        let repo_info = GitHubRepoInfo::from_remote_url(&pr_merge.pr_info.url)?;

        let pr_status = github_service
            .update_pr_status(&repo_info, pr_merge.pr_info.number)
            .await?;

        debug!(
            "PR #{} status: {:?} (was open)",
            pr_merge.pr_info.number, pr_status.status
        );

        // Update the PR status in the database
        if !matches!(&pr_status.status, MergeStatus::Open) {
            // Update merge status with the latest information from GitHub
            Merge::update_status(
                &self.db.pool,
                pr_merge.id,
                pr_status.status.clone(),
                pr_status.merge_commit_sha,
            )
            .await?;

            // If the PR was merged, update the task status to done
            if matches!(&pr_status.status, MergeStatus::Merged)
                && let Some(task_attempt) =
                    TaskAttempt::find_by_id(&self.db.pool, pr_merge.task_attempt_id).await?
            {
                info!(
                    "PR #{} was merged, updating task {} to done",
                    pr_merge.pr_info.number, task_attempt.task_id
                );
                Task::update_status(&self.db.pool, task_attempt.task_id, TaskStatus::Done).await?;

                if let Some(publisher) = &self.publisher
                    && let Err(err) = publisher
                        .update_shared_task_by_id(task_attempt.task_id)
                        .await
                {
                    tracing::warn!(
                        ?err,
                        "Failed to propagate shared task update for {}",
                        task_attempt.task_id
                    );
                }
            }
        }

        Ok(())
    }
}
