//! Background service for cleaning up stale node local projects.
//!
//! This service runs periodically and removes local projects that haven't been
//! reported by their node in a while. Only projects from online nodes are deleted,
//! since offline nodes may just need time to reconnect and re-sync their projects.

use chrono::Duration;
use sqlx::PgPool;
use std::time::Duration as StdDuration;
use tokio::time::{self, MissedTickBehavior};
use tracing::{debug, error, info};

use crate::db::node_local_projects::{NodeLocalProjectError, NodeLocalProjectRepository};

/// Configuration for the stale cleanup service.
#[derive(Debug, Clone)]
pub struct StaleCleanupConfig {
    /// How often to run the cleanup (default: 5 minutes)
    pub cleanup_interval: StdDuration,
    /// Duration after which a project is considered stale (default: 24 hours)
    ///
    /// Only projects from online nodes that haven't been seen in this duration
    /// will be deleted. Projects from offline nodes are not deleted since the
    /// node may reconnect and re-sync.
    pub stale_threshold: Duration,
}

impl Default for StaleCleanupConfig {
    fn default() -> Self {
        Self {
            cleanup_interval: StdDuration::from_secs(5 * 60), // 5 minutes
            stale_threshold: Duration::hours(24),             // 24 hours
        }
    }
}

/// Spawn the stale cleanup service as a background task.
///
/// This runs periodically and removes local projects from online nodes
/// that haven't been seen in the configured threshold duration.
pub fn spawn_stale_cleanup_service(pool: PgPool, config: Option<StaleCleanupConfig>) {
    let config = config.unwrap_or_default();

    tokio::spawn(async move {
        let mut interval = time::interval(config.cleanup_interval);
        interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

        loop {
            interval.tick().await;

            match cleanup_stale_projects(&pool, config.stale_threshold).await {
                Ok(deleted) => {
                    if deleted > 0 {
                        info!(
                            deleted = deleted,
                            threshold_hours = config.stale_threshold.num_hours(),
                            "Cleaned up stale node local projects"
                        );
                    } else {
                        debug!("No stale node local projects to clean up");
                    }
                }
                Err(e) => {
                    error!(error = ?e, "Failed to clean up stale node local projects");
                }
            }
        }
    });
}

/// Clean up stale projects from the database.
async fn cleanup_stale_projects(pool: &PgPool, threshold: Duration) -> Result<u64, sqlx::Error> {
    match NodeLocalProjectRepository::delete_stale(pool, threshold).await {
        Ok(deleted) => Ok(deleted),
        Err(NodeLocalProjectError::Database(e)) => Err(e),
        Err(NodeLocalProjectError::NotFound) => Ok(0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = StaleCleanupConfig::default();
        assert_eq!(config.cleanup_interval.as_secs(), 5 * 60);
        assert_eq!(config.stale_threshold.num_hours(), 24);
    }
}
