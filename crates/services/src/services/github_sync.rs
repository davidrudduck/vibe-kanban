use std::time::Duration;

use db::{DBService, models::project::Project};
use sqlx::error::Error as SqlxError;
use thiserror::Error;
use tokio::time::interval;
use tracing::{debug, error, info, warn};

use crate::services::github::{GitHubRepoInfo, GitHubService, GitHubServiceError};

#[derive(Debug, Error)]
pub enum GitHubSyncError {
    #[error(transparent)]
    GitHubServiceError(#[from] GitHubServiceError),
    #[error(transparent)]
    Sqlx(#[from] SqlxError),
}

/// Service to periodically sync GitHub issue/PR counts for projects with GitHub integration enabled
pub struct GitHubSyncService {
    db: DBService,
    poll_interval: Duration,
}

impl GitHubSyncService {
    /// Default poll interval of 5 minutes
    const DEFAULT_POLL_INTERVAL: Duration = Duration::from_secs(5 * 60);

    pub async fn spawn(db: DBService) -> tokio::task::JoinHandle<()> {
        Self::spawn_with_interval(db, Self::DEFAULT_POLL_INTERVAL).await
    }

    pub async fn spawn_with_interval(
        db: DBService,
        poll_interval: Duration,
    ) -> tokio::task::JoinHandle<()> {
        let service = Self { db, poll_interval };
        tokio::spawn(async move {
            service.start().await;
        })
    }

    async fn start(&self) {
        info!(
            "Starting GitHub sync service with interval {:?}",
            self.poll_interval
        );

        let mut interval = interval(self.poll_interval);

        // Run immediately on startup
        if let Err(e) = self.sync_all_projects().await {
            error!("Error during initial GitHub sync: {}", e);
        }

        loop {
            interval.tick().await;
            if let Err(e) = self.sync_all_projects().await {
                error!("Error syncing GitHub counts: {}", e);
            }
        }
    }

    /// Sync GitHub counts for all projects with GitHub integration enabled
    async fn sync_all_projects(&self) -> Result<(), GitHubSyncError> {
        let projects = Project::find_github_enabled(&self.db.pool).await?;

        if projects.is_empty() {
            debug!("No projects with GitHub integration enabled");
            return Ok(());
        }

        info!("Syncing GitHub counts for {} projects", projects.len());

        // Create GitHub service once (uses gh CLI)
        let github_service = match GitHubService::new() {
            Ok(service) => service,
            Err(e) => {
                warn!(
                    "Failed to create GitHub service: {}. GitHub sync disabled.",
                    e
                );
                return Ok(());
            }
        };

        for project in projects {
            if let Err(e) = self.sync_project(&github_service, &project).await {
                warn!(
                    project_id = %project.id,
                    project_name = %project.name,
                    "Error syncing GitHub counts: {}",
                    e
                );
            }
        }

        Ok(())
    }

    /// Sync GitHub counts for a single project
    async fn sync_project(
        &self,
        github_service: &GitHubService,
        project: &Project,
    ) -> Result<(), GitHubSyncError> {
        let owner = match &project.github_owner {
            Some(o) => o,
            None => {
                debug!(project_id = %project.id, "No GitHub owner set, skipping");
                return Ok(());
            }
        };
        let repo = match &project.github_repo {
            Some(r) => r,
            None => {
                debug!(project_id = %project.id, "No GitHub repo set, skipping");
                return Ok(());
            }
        };

        let repo_info = GitHubRepoInfo {
            owner: owner.clone(),
            repo_name: repo.clone(),
        };

        let counts = github_service.get_repo_counts(&repo_info).await?;

        debug!(
            project_id = %project.id,
            project_name = %project.name,
            owner = %owner,
            repo = %repo,
            open_issues = counts.open_issues,
            open_prs = counts.open_prs,
            "Synced GitHub counts"
        );

        // Update the database
        Project::update_github_counts(
            &self.db.pool,
            project.id,
            counts.open_issues,
            counts.open_prs,
        )
        .await?;

        Ok(())
    }
}

/// Trigger a sync for a single project (used when user enables GitHub integration)
pub async fn sync_single_project(db: &DBService, project: &Project) -> Result<(), GitHubSyncError> {
    let github_service = GitHubService::new()?;

    let owner = match &project.github_owner {
        Some(o) => o,
        None => return Ok(()),
    };
    let repo = match &project.github_repo {
        Some(r) => r,
        None => return Ok(()),
    };

    let repo_info = GitHubRepoInfo {
        owner: owner.clone(),
        repo_name: repo.clone(),
    };

    let counts = github_service.get_repo_counts(&repo_info).await?;

    Project::update_github_counts(&db.pool, project.id, counts.open_issues, counts.open_prs)
        .await?;

    info!(
        project_id = %project.id,
        open_issues = counts.open_issues,
        open_prs = counts.open_prs,
        "Synced GitHub counts for project"
    );

    Ok(())
}
