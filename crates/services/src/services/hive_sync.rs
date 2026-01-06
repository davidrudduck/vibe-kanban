//! Service for syncing local task attempts, execution processes, and logs to the Hive.
//!
//! This module provides a background sync service that periodically finds unsynced
//! entities in the local database and sends them to the Hive via WebSocket.
//!
//! # Sync Strategy
//!
//! The sync service uses the following strategy:
//! 1. **Task Sync**: First sync tasks (to get shared_task_id for attempts)
//! 2. **Attempt Sync**: Sync task attempts, as they are the parent entities
//! 3. **Execution Sync**: Sync execution processes, which belong to attempts
//! 4. **Log Batch Sync**: Finally sync log entries in batches, grouped by execution
//!
//! Entities are marked with `hive_synced_at` timestamp after successful sync.
//! On reconnection, all unsynced entities are retried.
//!
//! # Labels
//!
//! Labels are NOT synced from nodes to hive. Labels flow in one direction only:
//! from hive down to nodes. They are sent in the auth response and via broadcast
//! messages when updated on the hive.

use std::collections::HashMap;
use std::time::Duration;

use db::models::execution_process::ExecutionProcess;
use db::models::log_entry::DbLogEntry;
use db::models::project::Project;
use db::models::task::Task;
use db::models::task_attempt::TaskAttempt;
use sqlx::SqlitePool;
use tokio::sync::mpsc;
use tokio::time::{self, MissedTickBehavior};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use super::hive_client::{
    AttemptSyncMessage, ExecutionSyncMessage, LocalProjectSyncInfo, LogsBatchMessage, NodeMessage,
    ProjectsSyncMessage, SyncLogEntry, TaskOutputType, TaskSyncMessage,
};

/// Configuration for the Hive sync service.
#[derive(Debug, Clone)]
pub struct HiveSyncConfig {
    /// How often to check for unsynced entities
    pub sync_interval: Duration,
    /// Maximum number of tasks to sync in one batch
    pub max_tasks_per_batch: i64,
    /// Maximum number of attempts to sync in one batch
    pub max_attempts_per_batch: i64,
    /// Maximum number of executions to sync in one batch
    pub max_executions_per_batch: i64,
    /// Maximum number of log entries to sync in one batch
    pub max_logs_per_batch: i64,
}

impl Default for HiveSyncConfig {
    fn default() -> Self {
        Self {
            sync_interval: Duration::from_secs(5),
            max_tasks_per_batch: 50,
            max_attempts_per_batch: 50,
            max_executions_per_batch: 100,
            max_logs_per_batch: 500,
        }
    }
}

/// Errors from the Hive sync service.
#[derive(Debug, thiserror::Error)]
pub enum HiveSyncError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("send error: {0}")]
    Send(String),
    #[error("task not found for attempt: {0}")]
    TaskNotFound(Uuid),
    #[error("task not linked to Hive: {0}")]
    TaskNotLinked(Uuid),
}

/// Service for syncing local entities to the Hive.
pub struct HiveSyncService {
    pool: SqlitePool,
    command_tx: mpsc::Sender<NodeMessage>,
    config: HiveSyncConfig,
}

impl HiveSyncService {
    /// Create a new Hive sync service.
    pub fn new(
        pool: SqlitePool,
        command_tx: mpsc::Sender<NodeMessage>,
        config: HiveSyncConfig,
    ) -> Self {
        Self {
            pool,
            command_tx,
            config,
        }
    }

    /// Run the sync service in a loop.
    ///
    /// This spawns a background task that periodically syncs unsynced entities.
    /// The task runs until the channel is closed.
    pub fn spawn(self) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut interval = time::interval(self.config.sync_interval);
            interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

            loop {
                interval.tick().await;

                if let Err(e) = self.sync_once().await {
                    warn!(error = ?e, "Hive sync cycle failed");
                }
            }
        })
    }

    /// Perform one sync cycle.
    ///
    /// This syncs local projects first (so hive knows available projects), then
    /// tasks (to get shared_task_id), then attempts, executions, and logs in order.
    ///
    /// Note: Labels are NOT synced from nodes to hive. Labels are managed centrally
    /// on the hive and synced DOWN to nodes via the auth response and broadcast messages.
    pub async fn sync_once(&self) -> Result<(), HiveSyncError> {
        // Sync local projects first (so hive knows what's available on this node)
        if let Err(e) = self.sync_local_projects().await {
            // Log but don't fail the entire sync - other syncs can continue
            warn!(error = ?e, "Failed to sync local projects");
        }

        // Sync tasks first (to ensure shared_task_id exists for attempts)
        let tasks_synced = self.sync_tasks().await?;
        if tasks_synced > 0 {
            debug!(count = tasks_synced, "Synced tasks to Hive");
        }

        // Sync attempts next (parent entities for executions)
        let attempts_synced = self.sync_attempts().await?;
        if attempts_synced > 0 {
            debug!(count = attempts_synced, "Synced task attempts to Hive");
        }

        // Sync executions next
        let executions_synced = self.sync_executions().await?;
        if executions_synced > 0 {
            debug!(
                count = executions_synced,
                "Synced execution processes to Hive"
            );
        }

        // Sync logs
        let logs_synced = self.sync_logs().await?;
        if logs_synced > 0 {
            debug!(count = logs_synced, "Synced log entries to Hive");
        }

        // Labels are NOT synced from nodes to hive - they flow hive->nodes only

        Ok(())
    }

    /// Sync tasks that need a shared_task_id to the Hive.
    ///
    /// This finds tasks that:
    /// 1. Have unsynced attempts (attempts without hive_synced_at)
    /// 2. Don't have a shared_task_id
    /// 3. Belong to projects with a remote_project_id
    ///
    /// For each such task, we send a TaskSync message to the Hive.
    /// The Hive will respond with a TaskSyncResponse containing the shared_task_id.
    async fn sync_tasks(&self) -> Result<usize, HiveSyncError> {
        // Find tasks that need syncing: have unsynced attempts but no shared_task_id
        let tasks = Task::find_needing_sync(&self.pool, self.config.max_tasks_per_batch).await?;

        if tasks.is_empty() {
            return Ok(0);
        }

        let mut synced_count = 0;

        for task in &tasks {
            // Look up the project to get remote_project_id
            let project = match Project::find_by_id(&self.pool, task.project_id).await? {
                Some(p) => p,
                None => {
                    debug!(
                        task_id = %task.id,
                        project_id = %task.project_id,
                        "Skipping task sync - project not found"
                    );
                    continue;
                }
            };

            // Skip if project isn't linked to hive
            let remote_project_id = match project.remote_project_id {
                Some(id) => id,
                None => {
                    debug!(
                        task_id = %task.id,
                        project_id = %task.project_id,
                        "Skipping task sync - project not linked to Hive"
                    );
                    continue;
                }
            };

            // Serialize status to string
            let status = serde_json::to_value(&task.status)
                .ok()
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_else(|| "todo".to_string());

            let message = TaskSyncMessage {
                local_task_id: task.id,
                shared_task_id: task.shared_task_id,
                remote_project_id,
                title: task.title.clone(),
                description: task.description.clone(),
                status,
                version: 1, // Initial version for new sync
                is_update: task.shared_task_id.is_some(),
                created_at: task.created_at,
                updated_at: task.updated_at,
            };

            if let Err(e) = self.command_tx.send(NodeMessage::TaskSync(message)).await {
                error!(error = ?e, task_id = %task.id, "Failed to send task sync");
                return Err(HiveSyncError::Send(e.to_string()));
            }

            synced_count += 1;

            info!(
                task_id = %task.id,
                title = %task.title,
                remote_project_id = %remote_project_id,
                "Sent task sync to Hive"
            );
        }

        Ok(synced_count)
    }

    /// Sync unsynced task attempts to the Hive.
    async fn sync_attempts(&self) -> Result<usize, HiveSyncError> {
        let attempts =
            TaskAttempt::find_unsynced(&self.pool, self.config.max_attempts_per_batch).await?;

        if attempts.is_empty() {
            return Ok(0);
        }

        let mut synced_count = 0;
        let mut synced_ids = Vec::new();

        for attempt in &attempts {
            // Look up the task to get the shared_task_id
            let task = Task::find_by_id(&self.pool, attempt.task_id)
                .await?
                .ok_or(HiveSyncError::TaskNotFound(attempt.task_id))?;

            // Skip if task isn't linked to Hive
            let shared_task_id = match task.shared_task_id {
                Some(id) => id,
                None => {
                    debug!(
                        task_id = %attempt.task_id,
                        "Skipping attempt sync - task not linked to Hive"
                    );
                    continue;
                }
            };

            let message = AttemptSyncMessage {
                attempt_id: attempt.id,
                assignment_id: attempt.hive_assignment_id, // Use stored assignment_id if available
                shared_task_id,
                executor: attempt.executor.clone(),
                executor_variant: None, // TODO: Add executor_variant to TaskAttempt model
                branch: attempt.branch.clone(),
                target_branch: attempt.target_branch.clone(),
                container_ref: attempt.container_ref.clone(),
                worktree_deleted: attempt.worktree_deleted,
                setup_completed_at: attempt.setup_completed_at,
                created_at: attempt.created_at,
                updated_at: attempt.updated_at,
            };

            if let Err(e) = self
                .command_tx
                .send(NodeMessage::AttemptSync(message))
                .await
            {
                error!(error = ?e, attempt_id = %attempt.id, "Failed to send attempt sync");
                return Err(HiveSyncError::Send(e.to_string()));
            }

            synced_ids.push(attempt.id);
            synced_count += 1;
        }

        // Mark all synced attempts
        if !synced_ids.is_empty() {
            TaskAttempt::mark_hive_synced_batch(&self.pool, &synced_ids).await?;
        }

        Ok(synced_count)
    }

    /// Sync unsynced execution processes to the Hive.
    async fn sync_executions(&self) -> Result<usize, HiveSyncError> {
        let executions =
            ExecutionProcess::find_unsynced(&self.pool, self.config.max_executions_per_batch)
                .await?;

        if executions.is_empty() {
            return Ok(0);
        }

        let mut synced_count = 0;
        let mut synced_ids = Vec::new();

        for execution in &executions {
            // Serialize the executor_action to JSON
            let executor_action_json = serde_json::to_value(&execution.executor_action.0)
                .ok()
                .filter(|v| !v.is_null());

            // Serialize run_reason and status to their serde representations
            let run_reason = serde_json::to_value(&execution.run_reason)
                .ok()
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_else(|| "codingagent".to_string());
            let status = serde_json::to_value(&execution.status)
                .ok()
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_else(|| "running".to_string());

            let message = ExecutionSyncMessage {
                execution_id: execution.id,
                attempt_id: execution.task_attempt_id,
                run_reason,
                executor_action: executor_action_json,
                before_head_commit: execution.before_head_commit.clone(),
                after_head_commit: execution.after_head_commit.clone(),
                status,
                exit_code: execution.exit_code.map(|c| c as i32),
                dropped: execution.dropped,
                pid: execution.pid,
                started_at: execution.started_at,
                completed_at: execution.completed_at,
                created_at: execution.created_at,
            };

            if let Err(e) = self
                .command_tx
                .send(NodeMessage::ExecutionSync(message))
                .await
            {
                error!(error = ?e, execution_id = %execution.id, "Failed to send execution sync");
                return Err(HiveSyncError::Send(e.to_string()));
            }

            synced_ids.push(execution.id);
            synced_count += 1;
        }

        // Mark all synced executions
        if !synced_ids.is_empty() {
            ExecutionProcess::mark_hive_synced_batch(&self.pool, &synced_ids).await?;
        }

        Ok(synced_count)
    }

    /// Sync unsynced log entries to the Hive in batches.
    async fn sync_logs(&self) -> Result<usize, HiveSyncError> {
        let logs = DbLogEntry::find_unsynced(&self.pool, self.config.max_logs_per_batch).await?;

        if logs.is_empty() {
            return Ok(0);
        }

        // Group logs by execution_id for batching
        let mut batches: HashMap<Uuid, Vec<&DbLogEntry>> = HashMap::new();
        for log in &logs {
            batches.entry(log.execution_id).or_default().push(log);
        }

        let mut synced_count = 0;
        let mut synced_ids = Vec::new();

        for (execution_id, batch_logs) in batches {
            // Look up the execution process to get the task_attempt_id
            let execution = match ExecutionProcess::find_by_id(&self.pool, execution_id).await? {
                Some(ep) => ep,
                None => {
                    debug!(
                        execution_id = %execution_id,
                        "Skipping log sync - execution process not found"
                    );
                    continue;
                }
            };

            // Look up the task attempt to get hive_assignment_id
            let attempt =
                match TaskAttempt::find_by_id(&self.pool, execution.task_attempt_id).await? {
                    Some(ta) => ta,
                    None => {
                        debug!(
                            execution_id = %execution_id,
                            task_attempt_id = %execution.task_attempt_id,
                            "Skipping log sync - task attempt not found"
                        );
                        continue;
                    }
                };

            // Get the assignment_id from the attempt
            // For locally-started tasks, this will be None and we skip log sync
            // The Hive will create a synthetic assignment when it receives the AttemptSync
            let assignment_id = match attempt.hive_assignment_id {
                Some(id) => id,
                None => {
                    debug!(
                        execution_id = %execution_id,
                        attempt_id = %attempt.id,
                        "Skipping log sync - no Hive assignment (locally-started task)"
                    );
                    // Mark these logs as synced anyway to prevent retry spam
                    // They will be synced once the attempt gets a hive_assignment_id
                    // via AttemptSync -> synthetic assignment creation
                    for log in batch_logs {
                        synced_ids.push(log.id);
                        synced_count += 1;
                    }
                    continue;
                }
            };

            // Convert logs to sync format
            let entries: Vec<SyncLogEntry> = batch_logs
                .iter()
                .map(|log| SyncLogEntry {
                    output_type: parse_output_type(&log.output_type),
                    content: log.content.clone(),
                    timestamp: log.timestamp,
                })
                .collect();

            let message = LogsBatchMessage {
                assignment_id, // Now using the real assignment_id from the attempt
                execution_process_id: Some(execution_id),
                entries,
                compressed: false,
            };

            if let Err(e) = self.command_tx.send(NodeMessage::LogsBatch(message)).await {
                error!(error = ?e, execution_id = %execution_id, "Failed to send logs batch");
                return Err(HiveSyncError::Send(e.to_string()));
            }

            // Collect synced log IDs
            for log in batch_logs {
                synced_ids.push(log.id);
                synced_count += 1;
            }
        }

        // Mark all synced logs
        if !synced_ids.is_empty() {
            DbLogEntry::mark_hive_synced_batch(&self.pool, &synced_ids).await?;
        }

        Ok(synced_count)
    }

    // NOTE: sync_labels has been removed.
    // Labels are now managed centrally on the hive and synced DOWN to nodes.
    // See the auth response and HiveMessage::LabelSync broadcast for the new flow.

    /// Sync all local projects to the Hive.
    ///
    /// This sends a snapshot of all local projects to the Hive so it knows
    /// what projects are available on this node for linking to swarm projects.
    /// Called on each sync cycle to keep the Hive's view of node projects current.
    pub async fn sync_local_projects(&self) -> Result<usize, HiveSyncError> {
        // Fetch only local projects (is_remote=false) - don't sync remote projects back to hive
        let projects = Project::find_local_projects(&self.pool).await?;

        if projects.is_empty() {
            return Ok(0);
        }

        // Convert to sync format
        // Note: Project model doesn't have default_branch field, so we use "main" as default
        let project_infos: Vec<LocalProjectSyncInfo> = projects
            .iter()
            .map(|p| LocalProjectSyncInfo {
                local_project_id: p.id,
                name: p.name.clone(),
                git_repo_path: p.git_repo_path.to_string_lossy().into_owned(),
                default_branch: "main".to_string(),
            })
            .collect();

        let count = project_infos.len();

        let message = ProjectsSyncMessage {
            projects: project_infos,
        };

        if let Err(e) = self.command_tx.send(NodeMessage::ProjectsSync(message)).await {
            error!(error = ?e, "Failed to send projects sync");
            return Err(HiveSyncError::Send(e.to_string()));
        }

        debug!(count = count, "Synced local projects to Hive");
        Ok(count)
    }
}

/// Parse output type string to TaskOutputType.
fn parse_output_type(s: &str) -> TaskOutputType {
    match s.to_lowercase().as_str() {
        "stdout" => TaskOutputType::Stdout,
        "stderr" => TaskOutputType::Stderr,
        "system" => TaskOutputType::System,
        _ => TaskOutputType::System,
    }
}

/// Spawn the Hive sync service.
///
/// This is a convenience function that creates and spawns the sync service
/// in a background task.
pub fn spawn_hive_sync_service(
    pool: SqlitePool,
    command_tx: mpsc::Sender<NodeMessage>,
    config: Option<HiveSyncConfig>,
) -> tokio::task::JoinHandle<()> {
    let service = HiveSyncService::new(pool, command_tx, config.unwrap_or_default());
    service.spawn()
}
