//! Log migration service for JSONL to log_entries conversion.
//!
//! This module provides functionality to migrate execution logs from the legacy
//! `execution_process_logs` table (which stores batched JSONL) to the new
//! `log_entries` table (which stores individual rows). This migration is required
//! for ElectricSQL compatibility.
//!
//! ## Migration Process
//!
//! 1. Fetch all JSONL records from `execution_process_logs` for an execution
//! 2. Parse each JSONL line into a `LogMsg` enum
//! 3. Convert `LogMsg` to the unified `OutputType` format
//! 4. Insert individual entries into `log_entries` table
//! 5. Track migration progress (migrated, skipped, errors)
//!
//! ## Idempotency
//!
//! The migration is idempotent - running it multiple times will not create
//! duplicate entries. This is achieved by checking if entries already exist
//! before insertion.

use chrono::{DateTime, Utc};
use db::models::log_entry::{CreateLogEntry, DbLogEntry};
use sqlx::{Row, SqlitePool};
use thiserror::Error;
use tracing::{debug, error, info, warn};
use utils::log_msg::LogMsg;
use utils::unified_log::OutputType;
use uuid::Uuid;

/// Error types for log migration operations.
#[derive(Debug, Error)]
pub enum LogMigrationError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("failed to parse JSONL: {0}")]
    JsonParse(#[from] serde_json::Error),
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

/// Convert a LogMsg to (output_type, content) tuple.
fn log_msg_to_entry(log_msg: &LogMsg) -> (OutputType, String) {
    match log_msg {
        LogMsg::Stdout(s) => (OutputType::Stdout, s.clone()),
        LogMsg::Stderr(s) => (OutputType::Stderr, s.clone()),
        LogMsg::JsonPatch(patch) => {
            let content = serde_json::to_string(patch).unwrap_or_else(|_| "[]".to_string());
            (OutputType::JsonPatch, content)
        }
        LogMsg::SessionId(s) => (OutputType::SessionId, s.clone()),
        LogMsg::Finished => (OutputType::Finished, String::new()),
        LogMsg::RefreshRequired { reason } => (OutputType::RefreshRequired, reason.clone()),
    }
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

    let records = rows
        .iter()
        .map(|row| LegacyLogRecord {
            execution_id: row.get::<Uuid, _>("execution_id"),
            logs: row.get::<String, _>("logs"),
            byte_size: row.get::<i64, _>("byte_size"),
            inserted_at: row.get::<DateTime<Utc>, _>("inserted_at"),
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

/// Migrate logs for a single execution process.
///
/// This function reads all JSONL records from `execution_process_logs`,
/// parses each line, and inserts individual entries into `log_entries`.
///
/// The migration is idempotent - if entries already exist, they will be skipped.
pub async fn migrate_execution_logs(
    pool: &SqlitePool,
    execution_id: Uuid,
) -> Result<ExecutionMigrationResult, LogMigrationError> {
    let mut result = ExecutionMigrationResult::default();

    // Check if already migrated
    let existing_count = count_existing_entries(pool, execution_id).await?;
    if existing_count > 0 {
        // Count how many lines we have in the old table
        let records = fetch_legacy_logs(pool, execution_id).await?;
        let total_lines: usize = records
            .iter()
            .map(|r| r.logs.lines().filter(|l| !l.trim().is_empty()).count())
            .sum();

        if total_lines <= existing_count as usize {
            // All lines already migrated
            result.skipped = total_lines;
            debug!(
                execution_id = %execution_id,
                skipped = total_lines,
                "Execution already migrated, skipping"
            );
            return Ok(result);
        }
    }

    // Fetch legacy log records
    let records = fetch_legacy_logs(pool, execution_id).await?;

    if records.is_empty() {
        debug!(execution_id = %execution_id, "No legacy logs found for execution");
        return Ok(result);
    }

    // Process each record and line
    for record in &records {
        for line in record.logs.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            // Parse JSONL line
            match serde_json::from_str::<LogMsg>(line) {
                Ok(log_msg) => {
                    let (output_type, content) = log_msg_to_entry(&log_msg);

                    // Insert into log_entries
                    let create_entry = CreateLogEntry {
                        execution_id,
                        output_type: output_type.as_str().to_string(),
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
    if existing_count > 0 && existing_count as usize >= line_count {
        result.would_skip = line_count - result.errors;
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

/// Migrate all logs across all executions.
///
/// This function finds all executions with legacy logs and migrates them.
pub async fn migrate_all_logs(pool: &SqlitePool) -> Result<AllMigrationResult, LogMigrationError> {
    let mut result = AllMigrationResult::default();

    // Get all execution IDs with legacy logs
    let execution_ids = get_executions_with_legacy_logs(pool).await?;

    info!(
        count = execution_ids.len(),
        "Found executions with legacy logs"
    );

    for execution_id in execution_ids {
        result.executions_processed += 1;

        match migrate_execution_logs(pool, execution_id).await {
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

    #[test]
    fn test_log_msg_to_entry_stdout() {
        let msg = LogMsg::Stdout("hello".to_string());
        let (output_type, content) = log_msg_to_entry(&msg);
        assert_eq!(output_type, OutputType::Stdout);
        assert_eq!(content, "hello");
    }

    #[test]
    fn test_log_msg_to_entry_stderr() {
        let msg = LogMsg::Stderr("error".to_string());
        let (output_type, content) = log_msg_to_entry(&msg);
        assert_eq!(output_type, OutputType::Stderr);
        assert_eq!(content, "error");
    }

    #[test]
    fn test_log_msg_to_entry_session_id() {
        let msg = LogMsg::SessionId("abc123".to_string());
        let (output_type, content) = log_msg_to_entry(&msg);
        assert_eq!(output_type, OutputType::SessionId);
        assert_eq!(content, "abc123");
    }

    #[test]
    fn test_log_msg_to_entry_finished() {
        let msg = LogMsg::Finished;
        let (output_type, content) = log_msg_to_entry(&msg);
        assert_eq!(output_type, OutputType::Finished);
        assert!(content.is_empty());
    }

    #[test]
    fn test_log_msg_to_entry_refresh_required() {
        let msg = LogMsg::RefreshRequired {
            reason: "reconnect".to_string(),
        };
        let (output_type, content) = log_msg_to_entry(&msg);
        assert_eq!(output_type, OutputType::RefreshRequired);
        assert_eq!(content, "reconnect");
    }

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
}
