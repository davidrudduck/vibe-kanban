//! Log migration service for JSONL to log_entries conversion.
//!
//! This module provides functionality to migrate execution logs from the legacy
//! `execution_process_logs` table (which stores batched JSONL) to the new
//! `log_entries` table (which stores individual normalized rows).
//!
//! ## Migration Process
//!
//! 1. Fetch all JSONL records from `execution_process_logs` for an execution
//! 2. Parse each JSONL line into a `LogMsg` enum
//! 3. Create a temporary MsgStore and populate with Stdout/Stderr
//! 4. Run the executor's normalization logic to produce JsonPatch entries
//! 5. Insert normalized entries into `log_entries` table
//!
//! ## Idempotency
//!
//! The migration is idempotent - running it multiple times will not create
//! duplicate entries. This is achieved by checking if entries already exist
//! before insertion.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use db::models::execution_process::ExecutionProcess;
use db::models::log_entry::{CreateLogEntry, DbLogEntry};
use executors::actions::ExecutorActionType;
use executors::executors::StandardCodingAgentExecutor;
use executors::profile::ExecutorConfigs;
use json_patch::Patch;
use serde_json::Value;
use sqlx::{Row, SqlitePool};
use thiserror::Error;
use tokio::time::timeout;
use tracing::{debug, error, info, warn};
use utils::log_msg::LogMsg;
use utils::msg_store::MsgStore;
use utils::unified_log::OutputType;
use uuid::Uuid;

/// Error types for log migration operations.
#[derive(Debug, Error)]
pub enum LogMigrationError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("failed to parse JSONL: {0}")]
    JsonParse(#[from] serde_json::Error),

    #[error("execution process not found: {0}")]
    ExecutionNotFound(Uuid),

    #[error("invalid executor action")]
    InvalidExecutorAction,

    #[error("normalization timeout")]
    NormalizationTimeout,
}

/// Result of migrating logs for a single execution.
#[derive(Debug, Clone, Default)]
pub struct ExecutionMigrationResult {
    /// Number of log entries successfully migrated.
    pub migrated: usize,
    /// Number of log entries skipped (already exist).
    pub skipped: usize,
    /// Number of log entries that failed to parse/migrate.
    pub errors: usize,
}

/// Result of dry-run migration for a single execution.
#[derive(Debug, Clone, Default)]
pub struct DryRunResult {
    /// Number of log entries that would be migrated.
    pub would_migrate: usize,
    /// Number of log entries that would be skipped.
    pub would_skip: usize,
    /// Number of log entries with errors.
    pub errors: usize,
}

/// Result of migrating all logs across all executions.
#[derive(Debug, Clone, Default)]
pub struct AllMigrationResult {
    /// Total number of executions processed.
    pub executions_processed: usize,
    /// Total number of log entries migrated.
    pub total_migrated: usize,
    /// Total number of log entries skipped.
    pub total_skipped: usize,
    /// Total number of errors.
    pub total_errors: usize,
}

/// Legacy log record from execution_process_logs table.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct LegacyLogRecord {
    pub execution_id: Uuid,
    pub logs: String,
    pub byte_size: i64,
    pub inserted_at: DateTime<Utc>,
}

/// Fetch legacy log records for an execution.
pub async fn fetch_legacy_logs(
    pool: &SqlitePool,
    execution_id: Uuid,
) -> Result<Vec<LegacyLogRecord>, LogMigrationError> {
    let rows = sqlx::query(
        r#"SELECT execution_id, logs, byte_size, inserted_at
           FROM execution_process_logs
           WHERE execution_id = $1
           ORDER BY inserted_at ASC"#,
    )
    .bind(execution_id)
    .fetch_all(pool)
    .await?;

    let records: Vec<LegacyLogRecord> = rows
        .iter()
        .filter_map(|row| {
            let execution_id: Uuid = row.get("execution_id");
            match row.try_get::<DateTime<Utc>, _>("inserted_at") {
                Ok(inserted_at) => Some(LegacyLogRecord {
                    execution_id,
                    logs: row.get("logs"),
                    byte_size: row.get("byte_size"),
                    inserted_at,
                }),
                Err(e) => {
                    warn!(
                        execution_id = %execution_id,
                        error = %e,
                        "Skipping row with invalid inserted_at timestamp"
                    );
                    None
                }
            }
        })
        .collect();

    Ok(records)
}

/// Count existing log entries for an execution.
async fn count_existing_entries(pool: &SqlitePool, execution_id: Uuid) -> Result<i64, sqlx::Error> {
    let row = sqlx::query(r#"SELECT COUNT(*) as count FROM log_entries WHERE execution_id = $1"#)
        .bind(execution_id)
        .fetch_one(pool)
        .await?;

    Ok(row.get::<i64, _>("count"))
}

/// Delete existing log entries for an execution (for re-migration).
async fn delete_existing_entries(pool: &SqlitePool, execution_id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query(r#"DELETE FROM log_entries WHERE execution_id = $1"#)
        .bind(execution_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Migrate logs for a single execution process using normalization.
///
/// This function reads all JSONL records from `execution_process_logs`,
/// runs the executor's normalization logic to produce JsonPatch entries,
/// and inserts them into `log_entries`.
///
/// By default (incremental mode), already-migrated executions are skipped.
/// When `full_migration` is true, existing entries are deleted and re-migrated.
pub async fn migrate_execution_logs(
    pool: &SqlitePool,
    execution_id: Uuid,
) -> Result<ExecutionMigrationResult, LogMigrationError> {
    migrate_execution_logs_with_options(pool, execution_id, false).await
}

/// Migrate logs for a single execution with explicit full migration option.
///
/// When `full_migration` is true, existing entries are deleted and re-migrated.
/// When false (default), already-migrated executions are skipped.
pub async fn migrate_execution_logs_with_options(
    pool: &SqlitePool,
    execution_id: Uuid,
    full_migration: bool,
) -> Result<ExecutionMigrationResult, LogMigrationError> {
    let mut result = ExecutionMigrationResult::default();

    // Check if already migrated
    let existing_count = count_existing_entries(pool, execution_id).await?;
    if existing_count > 0 {
        if full_migration {
            // Full migration: delete existing entries and re-migrate
            debug!(
                execution_id = %execution_id,
                existing_count = existing_count,
                "Full migration: deleting existing entries for re-migration"
            );
            delete_existing_entries(pool, execution_id).await?;
        } else {
            // Incremental (default): skip already-migrated executions
            debug!(
                execution_id = %execution_id,
                existing_count = existing_count,
                "Execution already migrated, skipping"
            );
            result.skipped = 1; // One execution skipped
            return Ok(result);
        }
    }

    // Fetch legacy log records
    let records = fetch_legacy_logs(pool, execution_id).await?;

    if records.is_empty() {
        debug!(execution_id = %execution_id, "No legacy logs found for execution");
        return Ok(result);
    }

    // Get the execution process to determine executor type
    let process = ExecutionProcess::find_by_id(pool, execution_id)
        .await?
        .ok_or(LogMigrationError::ExecutionNotFound(execution_id))?;

    let executor_action = match process.executor_action() {
        Ok(action) => action,
        Err(e) => {
            debug!(
                execution_id = %execution_id,
                error = %e,
                "Could not parse executor action, skipping execution"
            );
            return Ok(result);
        }
    };

    // Get executor profile ID from the action
    let executor_profile_id = match executor_action.typ() {
        ExecutorActionType::CodingAgentInitialRequest(request) => &request.executor_profile_id,
        ExecutorActionType::CodingAgentFollowUpRequest(request) => &request.executor_profile_id,
        _ => {
            debug!(
                execution_id = %execution_id,
                "Executor action type doesn't support normalization, skipping"
            );
            return Ok(result);
        }
    };

    // Collect all messages, separating patches from raw logs
    let mut stdout_stderr_messages = Vec::new();
    let mut existing_patches: Vec<Patch> = Vec::new();

    for record in &records {
        for line in record.logs.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            // Parse JSONL line
            match serde_json::from_str::<LogMsg>(line) {
                Ok(log_msg) => {
                    match log_msg {
                        LogMsg::JsonPatch(patch) => {
                            existing_patches.push(patch);
                        }
                        LogMsg::Stdout(_) | LogMsg::Stderr(_) => {
                            stdout_stderr_messages.push(log_msg);
                        }
                        _ => {
                            // Ignore other message types
                        }
                    }
                }
                Err(e) => {
                    warn!(
                        execution_id = %execution_id,
                        line = line,
                        error = %e,
                        "Failed to parse JSONL line"
                    );
                    result.errors += 1;
                }
            }
        }
    }

    // Determine which path to take based on whether patches exist
    let patches: Vec<Patch> = if !existing_patches.is_empty() {
        debug!(
            execution_id = %execution_id,
            patch_count = existing_patches.len(),
            "Found existing JsonPatch entries, using them directly (no re-normalization)"
        );
        existing_patches
    } else if !stdout_stderr_messages.is_empty() {
        debug!(
            execution_id = %execution_id,
            message_count = stdout_stderr_messages.len(),
            "No existing patches found, normalizing stdout/stderr messages"
        );

        // Create temporary MsgStore and populate with stdout/stderr
        let msg_store = Arc::new(MsgStore::new());

        for log_msg in stdout_stderr_messages {
            msg_store.push(log_msg);
        }

        // Signal end of input
        msg_store.push_finished();

        // Get the executor and run normalization
        let executor = ExecutorConfigs::get_cached().get_coding_agent_or_default(executor_profile_id);

        debug!(
            execution_id = %execution_id,
            executor_type = ?executor_profile_id.executor,
            "Running normalization with executor"
        );

        // Use a placeholder worktree path since we don't have the actual directory
        // The path is only used for making file paths relative in tool output
        let worktree_path = PathBuf::from("/");

        executor.normalize_logs(msg_store.clone(), &worktree_path);

        // Wait for normalization to complete by polling until no new patches appear
        // The normalizer processes stdout_lines_stream and pushes JsonPatch entries to the store
        let collect_result = timeout(Duration::from_secs(30), async {
            // Give the normalizer task time to start and process
            // We poll the history until patch count stabilizes
            let mut last_count = 0;
            let mut stable_iterations = 0;

            loop {
                // Small delay to let normalizer process
                tokio::time::sleep(Duration::from_millis(50)).await;

                let current_patches: Vec<_> = msg_store
                    .get_history()
                    .into_iter()
                    .filter(|msg| matches!(msg, LogMsg::JsonPatch(_)))
                    .collect();

                let current_count = current_patches.len();

                if current_count == last_count {
                    stable_iterations += 1;
                    // If count is stable for 3 iterations (150ms), normalization is likely done
                    if stable_iterations >= 3 {
                        debug!(
                            execution_id = %execution_id,
                            patch_count = current_count,
                            "Normalization complete, patch count stable"
                        );
                        return current_patches
                            .into_iter()
                            .filter_map(|msg| {
                                if let LogMsg::JsonPatch(patch) = msg {
                                    Some(patch)
                                } else {
                                    None
                                }
                            })
                            .collect();
                    }
                } else {
                    stable_iterations = 0;
                    last_count = current_count;
                }
            }
        })
        .await;

        match collect_result {
            Ok(patches) => patches,
            Err(_) => {
                error!(
                    execution_id = %execution_id,
                    "Normalization timed out after 30 seconds"
                );
                return Err(LogMigrationError::NormalizationTimeout);
            }
        }
    } else {
        debug!(execution_id = %execution_id, "No log messages found to process");
        return Ok(result);
    };

    debug!(
        execution_id = %execution_id,
        patch_count = patches.len(),
        "Collected normalized patches"
    );

    // Extract final entries directly from patches instead of applying sequentially.
    // This avoids errors when "replace" operations target indices that don't exist yet
    // (which happens when streaming content creates add+replace sequences).
    // We build a map of id -> latest value, then sort by index.
    let mut entry_map: BTreeMap<usize, Value> = BTreeMap::new();

    for patch in &patches {
        // Each patch is a Vec of operations; extract the value from add/replace ops
        let Ok(ops) = serde_json::to_value(patch) else {
            continue;
        };
        let Some(ops_array) = ops.as_array() else {
            continue;
        };
        for op in ops_array {
            let op_type = op.get("op").and_then(|v| v.as_str());
            let path = op.get("path").and_then(|v| v.as_str());
            let value = op.get("value");

            // Only process add/replace operations on /entries/{index}
            if matches!(op_type, Some("add") | Some("replace"))
                && let Some(path_str) = path
                && let Some(idx_str) = path_str.strip_prefix("/entries/")
                && let Ok(idx) = idx_str.parse::<usize>()
                && let Some(val) = value
            {
                entry_map.insert(idx, val.clone());
            }
        }
    }

    // Convert the map to a sorted vector of entries
    let entries: Vec<Value> = entry_map.into_values().collect();

    debug!(
        execution_id = %execution_id,
        patch_count = patches.len(),
        final_entries = entries.len(),
        "Applied patches to build final entries"
    );

    // Insert each final entry into log_entries as an "add" patch operation
    // Each database row = one conversation message wrapped in a patch that appends it
    // Using "/entries/-" appends to the array (RFC 6902)
    for entry in entries {
        // Wrap the entry in an "add" operation that appends to the entries array
        let add_patch = serde_json::json!([{
            "op": "add",
            "path": "/entries/-",
            "value": entry
        }]);
        let content = serde_json::to_string(&add_patch).unwrap_or_else(|_| "[]".to_string());
        let create_entry = CreateLogEntry {
            execution_id,
            output_type: OutputType::JsonPatch.as_str().to_string(),
            content,
        };

        match DbLogEntry::create(pool, create_entry).await {
            Ok(_) => {
                result.migrated += 1;
            }
            Err(e) => {
                error!(
                    execution_id = %execution_id,
                    error = %e,
                    "Failed to insert log entry"
                );
                result.errors += 1;
            }
        }
    }

    info!(
        execution_id = %execution_id,
        migrated = result.migrated,
        skipped = result.skipped,
        errors = result.errors,
        "Migration complete for execution"
    );

    Ok(result)
}

/// Dry-run migration for a single execution (no database writes).
///
/// This function simulates the migration and reports what would happen
/// without actually inserting any entries.
pub async fn migrate_execution_logs_dry_run(
    pool: &SqlitePool,
    execution_id: Uuid,
) -> Result<DryRunResult, LogMigrationError> {
    let mut result = DryRunResult::default();

    // Check if already migrated
    let existing_count = count_existing_entries(pool, execution_id).await?;

    // Fetch legacy log records
    let records = fetch_legacy_logs(pool, execution_id).await?;

    if records.is_empty() {
        return Ok(result);
    }

    let mut line_count = 0;

    // Process each record and line
    for record in &records {
        for line in record.logs.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            line_count += 1;

            // Parse JSONL line
            match serde_json::from_str::<LogMsg>(line) {
                Ok(_) => {
                    // Would be migrated
                }
                Err(_) => {
                    result.errors += 1;
                }
            }
        }
    }

    // Calculate what would be migrated vs skipped
    if existing_count > 0 {
        result.would_skip = 0; // We would delete and re-migrate
        result.would_migrate = line_count - result.errors;
    } else {
        result.would_migrate = line_count - result.errors;
    }

    Ok(result)
}

/// Get all execution IDs that have legacy logs.
pub async fn get_executions_with_legacy_logs(pool: &SqlitePool) -> Result<Vec<Uuid>, sqlx::Error> {
    let rows = sqlx::query(
        r#"SELECT DISTINCT execution_id
           FROM execution_process_logs
           ORDER BY execution_id"#,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| r.get::<Uuid, _>("execution_id"))
        .collect())
}

/// Find executions that may have incomplete log data due to server shutdown.
///
/// Returns executions where:
/// - execution_process_logs has records (JSONL backup exists)
/// - log_entries is empty (migration never completed)
/// - execution status is terminal (completed, failed, killed)
///
/// These are candidates for auto-recovery on startup.
pub async fn find_incomplete_executions(pool: &SqlitePool) -> Result<Vec<Uuid>, sqlx::Error> {
    let rows = sqlx::query(
        r#"SELECT DISTINCT epl.execution_id
           FROM execution_process_logs epl
           INNER JOIN execution_processes ep ON epl.execution_id = ep.id
           WHERE ep.dropped = FALSE
             AND ep.status IN ('completed', 'failed', 'killed')
             AND NOT EXISTS (
                 SELECT 1 FROM log_entries le
                 WHERE le.execution_id = epl.execution_id
             )
           ORDER BY ep.completed_at DESC"#,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| r.get::<Uuid, _>("execution_id"))
        .collect())
}

/// Recover incomplete executions by migrating their logs from JSONL to log_entries.
///
/// This should be called on server startup to recover any data that wasn't
/// properly migrated due to abrupt shutdown.
pub async fn recover_incomplete_executions(
    pool: &SqlitePool,
) -> Result<AllMigrationResult, LogMigrationError> {
    let mut result = AllMigrationResult::default();

    let incomplete_ids = find_incomplete_executions(pool).await?;

    if incomplete_ids.is_empty() {
        debug!("No incomplete executions found, nothing to recover");
        return Ok(result);
    }

    info!(
        count = incomplete_ids.len(),
        "Found incomplete executions, recovering..."
    );

    for execution_id in incomplete_ids {
        result.executions_processed += 1;

        match migrate_execution_logs(pool, execution_id).await {
            Ok(exec_result) => {
                result.total_migrated += exec_result.migrated;
                result.total_skipped += exec_result.skipped;
                result.total_errors += exec_result.errors;

                debug!(
                    execution_id = %execution_id,
                    migrated = exec_result.migrated,
                    "Recovered execution logs"
                );
            }
            Err(e) => {
                warn!(
                    execution_id = %execution_id,
                    error = %e,
                    "Failed to recover execution logs"
                );
                result.total_errors += 1;
            }
        }
    }

    info!(
        recovered = result.executions_processed,
        migrated = result.total_migrated,
        errors = result.total_errors,
        "Log recovery complete"
    );

    Ok(result)
}

/// Migrate all logs across all executions (incremental by default).
///
/// This function finds all executions with legacy logs and migrates them.
/// Already-migrated executions are skipped in incremental mode.
pub async fn migrate_all_logs(pool: &SqlitePool) -> Result<AllMigrationResult, LogMigrationError> {
    migrate_all_logs_with_options(pool, false).await
}

/// Migrate all logs with explicit full migration option.
///
/// When `full_migration` is true, existing entries are deleted and re-migrated.
/// When false (default), already-migrated executions are skipped.
pub async fn migrate_all_logs_with_options(
    pool: &SqlitePool,
    full_migration: bool,
) -> Result<AllMigrationResult, LogMigrationError> {
    let mut result = AllMigrationResult::default();

    // Get all execution IDs with legacy logs
    let execution_ids = get_executions_with_legacy_logs(pool).await?;

    info!(
        count = execution_ids.len(),
        full_migration = full_migration,
        "Found executions with legacy logs"
    );

    for execution_id in execution_ids {
        result.executions_processed += 1;

        match migrate_execution_logs_with_options(pool, execution_id, full_migration).await {
            Ok(exec_result) => {
                result.total_migrated += exec_result.migrated;
                result.total_skipped += exec_result.skipped;
                result.total_errors += exec_result.errors;
            }
            Err(e) => {
                error!(
                    execution_id = %execution_id,
                    error = %e,
                    "Failed to migrate execution"
                );
                result.total_errors += 1;
            }
        }

        // Log progress every 10 executions
        if result.executions_processed % 10 == 0 {
            info!(
                processed = result.executions_processed,
                migrated = result.total_migrated,
                "Migration progress"
            );
        }
    }

    info!(
        executions = result.executions_processed,
        migrated = result.total_migrated,
        skipped = result.total_skipped,
        errors = result.total_errors,
        "All migrations complete"
    );

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use db::test_utils::create_test_pool;
    use executors::actions::{coding_agent_initial::CodingAgentInitialRequest, ExecutorAction, ExecutorActionType};
    use executors::executors::BaseCodingAgent;
    use executors::profile::ExecutorProfileId;
    use serde_json::json;

    #[test]
    fn test_execution_migration_result_default() {
        let result = ExecutionMigrationResult::default();
        assert_eq!(result.migrated, 0);
        assert_eq!(result.skipped, 0);
        assert_eq!(result.errors, 0);
    }

    #[test]
    fn test_all_migration_result_default() {
        let result = AllMigrationResult::default();
        assert_eq!(result.executions_processed, 0);
        assert_eq!(result.total_migrated, 0);
        assert_eq!(result.total_skipped, 0);
        assert_eq!(result.total_errors, 0);
    }

    #[tokio::test]
    async fn test_migrate_execution_logs_no_duplicates() {
        let (pool, _temp_dir) = create_test_pool().await;

        // Setup: Create execution_process with minimal dependencies by directly inserting
        let project_id = Uuid::new_v4();
        let task_id = Uuid::new_v4();
        let task_attempt_id = Uuid::new_v4();
        let execution_id = Uuid::new_v4();

        // Insert minimal project record
        sqlx::query("INSERT INTO projects (id, name) VALUES ($1, $2)")
            .bind(project_id)
            .bind("Test Project")
            .execute(&pool)
            .await
            .expect("Failed to insert project");

        // Insert minimal task record
        sqlx::query("INSERT INTO tasks (id, project_id, title) VALUES ($1, $2, $3)")
            .bind(task_id)
            .bind(project_id)
            .bind("Test Task")
            .execute(&pool)
            .await
            .expect("Failed to insert task");

        // Insert minimal task_attempt record
        sqlx::query(
            "INSERT INTO task_attempts (id, task_id, branch, target_branch, executor) VALUES ($1, $2, $3, $4, $5)"
        )
        .bind(task_attempt_id)
        .bind(task_id)
        .bind("test-branch")
        .bind("main")
        .bind("claude_code")
        .execute(&pool)
        .await
        .expect("Failed to insert task_attempt");

        // Create executor action
        let executor_action = ExecutorAction::new(
            ExecutorActionType::CodingAgentInitialRequest(CodingAgentInitialRequest {
                executor_profile_id: ExecutorProfileId::new(BaseCodingAgent::ClaudeCode),
                prompt: "Test prompt".to_string(),
            }),
            None,
        );
        let executor_action_json = serde_json::to_string(&executor_action).unwrap();

        // Insert minimal execution_process record
        sqlx::query(
            r#"INSERT INTO execution_processes
               (id, task_attempt_id, run_reason, executor_action, status, started_at)
               VALUES ($1, $2, $3, $4, $5, $6)"#,
        )
        .bind(execution_id)
        .bind(task_attempt_id)
        .bind("codingagent")
        .bind(executor_action_json)
        .bind("completed")
        .bind(Utc::now())
        .execute(&pool)
        .await
        .expect("Failed to insert execution_process");

        // Create legacy logs with existing JsonPatch entries (already normalized)
        let patch1 = json!([{
            "op": "add",
            "path": "/entries/0",
            "value": {
                "type": "assistant_message",
                "content": "Test message 1"
            }
        }]);

        let patch2 = json!([{
            "op": "add",
            "path": "/entries/1",
            "value": {
                "type": "assistant_message",
                "content": "Test message 2"
            }
        }]);

        // Create JSONL with both JsonPatch and Stdout entries
        let jsonl = format!(
            "{}\n{}\n{}",
            serde_json::to_string(&LogMsg::JsonPatch(
                serde_json::from_value(patch1).expect("Failed to create patch1")
            ))
            .expect("Failed to serialize patch1"),
            serde_json::to_string(&LogMsg::JsonPatch(
                serde_json::from_value(patch2).expect("Failed to create patch2")
            ))
            .expect("Failed to serialize patch2"),
            serde_json::to_string(&LogMsg::Stdout("stdout line".to_string()))
                .expect("Failed to serialize stdout")
        );

        // Insert legacy log record
        sqlx::query(
            r#"INSERT INTO execution_process_logs (execution_id, logs, byte_size, inserted_at)
               VALUES ($1, $2, $3, $4)"#,
        )
        .bind(execution_id)
        .bind(&jsonl)
        .bind(jsonl.len() as i64)
        .bind(Utc::now())
        .execute(&pool)
        .await
        .expect("Failed to insert legacy log record");

        // Run migration
        let result = migrate_execution_logs_with_options(&pool, execution_id, false)
            .await
            .expect("Migration failed");

        // Assert: log_entries count should match original JsonPatch count (2)
        // The migration should not create duplicates from the stdout line
        let count: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*) FROM log_entries WHERE execution_id = $1"#
        )
        .bind(execution_id)
        .fetch_one(&pool)
        .await
        .expect("Failed to count log entries");

        assert_eq!(
            count, 2,
            "Expected 2 log entries (matching original JsonPatch count), no duplicates"
        );
        assert_eq!(
            result.migrated, 2,
            "Expected 2 migrated entries"
        );
        assert_eq!(
            result.errors, 0,
            "Expected no errors"
        );
    }

    #[tokio::test]
    async fn test_migrate_execution_logs_idempotent() {
        let (pool, _temp_dir) = create_test_pool().await;

        // Setup: Create execution_process with minimal dependencies
        let project_id = Uuid::new_v4();
        let task_id = Uuid::new_v4();
        let task_attempt_id = Uuid::new_v4();
        let execution_id = Uuid::new_v4();

        // Insert minimal project record
        sqlx::query("INSERT INTO projects (id, name) VALUES ($1, $2)")
            .bind(project_id)
            .bind("Test Project")
            .execute(&pool)
            .await
            .expect("Failed to insert project");

        // Insert minimal task record
        sqlx::query("INSERT INTO tasks (id, project_id, title) VALUES ($1, $2, $3)")
            .bind(task_id)
            .bind(project_id)
            .bind("Test Task")
            .execute(&pool)
            .await
            .expect("Failed to insert task");

        // Insert minimal task_attempt record
        sqlx::query(
            "INSERT INTO task_attempts (id, task_id, branch, target_branch, executor) VALUES ($1, $2, $3, $4, $5)"
        )
        .bind(task_attempt_id)
        .bind(task_id)
        .bind("test-branch")
        .bind("main")
        .bind("claude_code")
        .execute(&pool)
        .await
        .expect("Failed to insert task_attempt");

        // Create executor action
        let executor_action = ExecutorAction::new(
            ExecutorActionType::CodingAgentInitialRequest(CodingAgentInitialRequest {
                executor_profile_id: ExecutorProfileId::new(BaseCodingAgent::ClaudeCode),
                prompt: "Test prompt".to_string(),
            }),
            None,
        );
        let executor_action_json = serde_json::to_string(&executor_action).unwrap();

        // Insert minimal execution_process record
        sqlx::query(
            r#"INSERT INTO execution_processes
               (id, task_attempt_id, run_reason, executor_action, status, started_at)
               VALUES ($1, $2, $3, $4, $5, $6)"#,
        )
        .bind(execution_id)
        .bind(task_attempt_id)
        .bind("codingagent")
        .bind(executor_action_json)
        .bind("completed")
        .bind(Utc::now())
        .execute(&pool)
        .await
        .expect("Failed to insert execution_process");

        // Create legacy logs with existing JsonPatch entries
        let patch1 = json!([{
            "op": "add",
            "path": "/entries/0",
            "value": {
                "type": "assistant_message",
                "content": "Idempotency test message 1"
            }
        }]);

        let patch2 = json!([{
            "op": "add",
            "path": "/entries/1",
            "value": {
                "type": "assistant_message",
                "content": "Idempotency test message 2"
            }
        }]);

        let jsonl = format!(
            "{}\n{}",
            serde_json::to_string(&LogMsg::JsonPatch(
                serde_json::from_value(patch1).expect("Failed to create patch1")
            ))
            .expect("Failed to serialize patch1"),
            serde_json::to_string(&LogMsg::JsonPatch(
                serde_json::from_value(patch2).expect("Failed to create patch2")
            ))
            .expect("Failed to serialize patch2")
        );

        // Insert legacy log record
        sqlx::query(
            r#"INSERT INTO execution_process_logs (execution_id, logs, byte_size, inserted_at)
               VALUES ($1, $2, $3, $4)"#,
        )
        .bind(execution_id)
        .bind(&jsonl)
        .bind(jsonl.len() as i64)
        .bind(Utc::now())
        .execute(&pool)
        .await
        .expect("Failed to insert legacy log record");

        // Run migration first time
        let result1 = migrate_execution_logs_with_options(&pool, execution_id, false)
            .await
            .expect("First migration failed");

        // Get log entries after first migration
        let entries_after_first: Vec<(i64, String)> = sqlx::query_as(
            r#"SELECT id, content FROM log_entries WHERE execution_id = $1 ORDER BY id"#
        )
        .bind(execution_id)
        .fetch_all(&pool)
        .await
        .expect("Failed to fetch log entries after first migration");

        let count_after_first = entries_after_first.len();

        // Run migration second time
        let result2 = migrate_execution_logs_with_options(&pool, execution_id, false)
            .await
            .expect("Second migration failed");

        // Get log entries after second migration
        let entries_after_second: Vec<(i64, String)> = sqlx::query_as(
            r#"SELECT id, content FROM log_entries WHERE execution_id = $1 ORDER BY id"#
        )
        .bind(execution_id)
        .fetch_all(&pool)
        .await
        .expect("Failed to fetch log entries after second migration");

        let count_after_second = entries_after_second.len();

        // Assert: Count should remain the same after second run (idempotent)
        assert_eq!(
            count_after_first, count_after_second,
            "Expected same count after second migration (idempotent)"
        );

        assert_eq!(
            count_after_first, 2,
            "Expected 2 log entries after first migration"
        );

        assert_eq!(
            count_after_second, 2,
            "Expected 2 log entries after second migration (no duplicates)"
        );

        // Assert: Content should be identical
        for (i, (first, second)) in entries_after_first.iter().zip(entries_after_second.iter()).enumerate() {
            assert_eq!(
                first.0, second.0,
                "Entry index {} should match between runs", i
            );
            assert_eq!(
                first.1, second.1,
                "Content at index {} should match between runs", i
            );
        }

        // Assert: First run should have migrated 2 entries
        assert_eq!(
            result1.migrated, 2,
            "Expected 2 entries migrated in first run"
        );

        // Assert: Second run should skip (already migrated)
        assert_eq!(
            result2.skipped, 1,
            "Expected execution to be skipped in second run (already migrated)"
        );

        assert_eq!(
            result2.migrated, 0,
            "Expected 0 entries migrated in second run (already complete)"
        );
    }
}
