//! Service for syncing local task attempts, execution processes, and logs to the Hive.
//!
//! This module provides a background sync service that periodically finds unsynced
//! entities in the local database and sends them to the Hive via WebSocket.
//!
//! # Sync Strategy
//!
//! The sync service uses the following strategy:
//! 1. **Attempt Sync**: First sync task attempts, as they are the parent entities
//! 2. **Execution Sync**: Then sync execution processes, which belong to attempts
//! 3. **Log Batch Sync**: Finally sync log entries in batches, grouped by execution
//!
//! Entities are marked with `hive_synced_at` timestamp after successful sync.
//! On reconnection, all unsynced entities are retried.

use std::collections::HashMap;
use std::time::Duration;

use db::models::execution_process::ExecutionProcess;
use db::models::label::Label;
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
    AttemptSyncMessage, ExecutionSyncMessage, LabelSyncMessage, LogsBatchMessage, NodeMessage,
    SyncLogEntry, TaskOutputType, TaskSyncMessage,
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
    /// Maximum number of labels to sync in one batch
    pub max_labels_per_batch: i64,
}

impl Default for HiveSyncConfig {
    fn default() -> Self {
        Self {
            sync_interval: Duration::from_secs(5),
            max_tasks_per_batch: 50,
            max_attempts_per_batch: 50,
            max_executions_per_batch: 100,
            max_logs_per_batch: 500,
            max_labels_per_batch: 50,
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
    /// This syncs tasks first (to get swarm_task_id), then attempts, executions,
    /// logs, and labels in order.
    pub async fn sync_once(&self) -> Result<(), HiveSyncError> {
        // Sync tasks first (to ensure swarm_task_id exists for attempts)
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

        // Sync labels
        let labels_synced = self.sync_labels().await?;
        if labels_synced > 0 {
            debug!(count = labels_synced, "Synced labels to Hive");
        }

        Ok(())
    }

    /// Sync tasks that need a swarm_task_id to the Hive.
    ///
    /// This finds tasks that:
    /// 1. Have unsynced attempts (attempts without hive_synced_at)
    /// 2. Don't have a swarm_task_id
    /// 3. Belong to projects with a swarm_project_id
    ///
    /// For each such task, we send a TaskSync message to the Hive.
    /// The Hive will respond with a TaskSyncResponse containing the swarm_task_id.
    async fn sync_tasks(&self) -> Result<usize, HiveSyncError> {
        // Find tasks that need syncing: have unsynced attempts but no swarm_task_id
        let tasks = Task::find_needing_sync(&self.pool, self.config.max_tasks_per_batch).await?;

        if tasks.is_empty() {
            return Ok(0);
        }

        let mut synced_count = 0;

        for task in &tasks {
            // Look up the project to get swarm_project_id
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
            let swarm_project_id = match project.swarm_project_id {
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
                swarm_task_id: task.swarm_task_id,
                swarm_project_id,
                title: task.title.clone(),
                description: task.description.clone(),
                status,
                version: 1, // Initial version for new sync
                is_update: task.swarm_task_id.is_some(),
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
                swarm_project_id = %swarm_project_id,
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
            // Look up the task to get the swarm_task_id
            let task = Task::find_by_id(&self.pool, attempt.task_id)
                .await?
                .ok_or(HiveSyncError::TaskNotFound(attempt.task_id))?;

            // Skip if task isn't linked to Hive
            let swarm_task_id = match task.swarm_task_id {
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
                swarm_task_id,
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
            let attempt = match TaskAttempt::find_by_id(&self.pool, execution.task_attempt_id).await? {
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

    /// Sync unsynced labels to the Hive.
    ///
    /// This syncs both new labels (no swarm_label_id) and modified labels
    /// (updated_at > synced_at).
    async fn sync_labels(&self) -> Result<usize, HiveSyncError> {
        // First, get labels that have never been synced
        let unsynced_labels = Label::find_unsynced(&self.pool).await?;

        // Then, get labels that have been modified since last sync
        let modified_labels = Label::find_modified_since_sync(&self.pool).await?;

        // Combine and deduplicate (prefer modified version if label appears in both)
        let mut labels_to_sync: HashMap<Uuid, (Label, bool)> = HashMap::new();

        // New labels (is_update = false)
        for label in unsynced_labels
            .into_iter()
            .take(self.config.max_labels_per_batch as usize)
        {
            labels_to_sync.insert(label.id, (label, false));
        }

        // Modified labels (is_update = true) - these take precedence
        for label in modified_labels
            .into_iter()
            .take(self.config.max_labels_per_batch as usize)
        {
            labels_to_sync.insert(label.id, (label, true));
        }

        if labels_to_sync.is_empty() {
            return Ok(0);
        }

        let mut synced_count = 0;
        let mut synced_ids = Vec::new();

        for (label_id, (label, is_update)) in labels_to_sync {
            // Look up the project to get swarm_project_id (if this is a project-specific label)
            let swarm_project_id = if let Some(project_id) = label.project_id {
                match Project::find_by_id(&self.pool, project_id).await? {
                    Some(project) => project.swarm_project_id,
                    None => {
                        debug!(
                            label_id = %label_id,
                            project_id = %project_id,
                            "Skipping label sync - project not found"
                        );
                        continue;
                    }
                }
            } else {
                // Global label - no project association
                None
            };

            let message = LabelSyncMessage {
                label_id: label.id,
                swarm_label_id: label.swarm_label_id,
                project_id: label.project_id,
                swarm_project_id,
                name: label.name.clone(),
                icon: label.icon.clone(),
                color: label.color.clone(),
                version: label.version,
                is_update,
            };

            if let Err(e) = self
                .command_tx
                .send(NodeMessage::LabelSync(message))
                .await
            {
                error!(error = ?e, label_id = %label_id, "Failed to send label sync");
                return Err(HiveSyncError::Send(e.to_string()));
            }

            synced_ids.push(label_id);
            synced_count += 1;

            info!(
                label_id = %label_id,
                name = %label.name,
                is_update = is_update,
                "Synced label to Hive"
            );
        }

        // Mark all synced labels
        // Note: For new labels, the Hive will respond with the swarm_label_id
        // which will be set via Label::set_swarm_label_id
        // For updated labels, we just mark them as synced
        for label_id in &synced_ids {
            Label::mark_synced(&self.pool, *label_id).await?;
        }

        Ok(synced_count)
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
