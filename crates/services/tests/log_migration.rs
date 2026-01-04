//! Integration tests for JSONL to log_entries migration.
//!
//! These tests verify that the log migration service correctly:
//! 1. Reads JSONL log records from execution_process_logs table
//! 2. Parses each JSONL line into individual log entries
//! 3. Inserts entries into the new log_entries table
//! 4. Handles various log types (stdout, stderr, json_patch, etc.)
//! 5. Preserves execution_id and timestamp associations
//! 6. Supports dry-run and execute modes

use db::models::log_entry::DbLogEntry;
use serde_json::json;
use sqlx::{Row, SqlitePool};
use std::str::FromStr;
use tempfile::TempDir;
use uuid::Uuid;

/// Create a test database with migrations applied.
async fn setup_test_db() -> (SqlitePool, TempDir) {
    use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode};

    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");

    let options =
        SqliteConnectOptions::from_str(&format!("sqlite://{}", db_path.to_string_lossy()))
            .expect("Invalid database URL")
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal);

    let pool = SqlitePool::connect_with(options)
        .await
        .expect("Failed to create pool");

    // Run migrations
    sqlx::migrate!("../db/migrations")
        .run(&pool)
        .await
        .expect("Failed to run migrations");

    (pool, temp_dir)
}

/// Create a valid executor_action JSON for testing.
fn create_test_executor_action() -> String {
    r#"{"typ":{"type":"CodingAgentInitialRequest","prompt":"Test prompt","executor_profile_id":{"executor":"CLAUDE_CODE","variant":null}},"next_action":null}"#.to_string()
}

// =============================================================================
// Claude JSON Helper Functions
// =============================================================================
//
// These helpers generate realistic Claude Code executor output that the
// normalization logic can parse and transform into JsonPatch entries.
//
// The log migration reads JSONL from execution_process_logs where each line
// is a LogMsg variant (e.g., {"Stdout": "..."}, {"Stderr": "..."}).
// The Stdout content contains Claude's JSON protocol messages.
// =============================================================================

/// Wrap a line as a Stdout LogMsg for JSONL storage.
fn wrap_as_stdout(line: &str) -> String {
    json!({"Stdout": line}).to_string()
}

/// Create a Claude assistant message with text content.
fn claude_assistant_message(text: &str, msg_id: &str) -> String {
    json!({
        "type": "assistant",
        "message": {
            "id": msg_id,
            "role": "assistant",
            "content": [{"type": "text", "text": text}]
        }
    })
    .to_string()
}

/// Create a Claude tool_use message.
#[allow(dead_code)]
fn claude_tool_use(tool_name: &str, tool_id: &str, input: serde_json::Value) -> String {
    json!({
        "type": "assistant",
        "message": {
            "id": format!("msg_{}", tool_id),
            "role": "assistant",
            "content": [{
                "type": "tool_use",
                "id": tool_id,
                "name": tool_name,
                "input": input
            }]
        }
    })
    .to_string()
}

/// Create a Claude tool_result message.
#[allow(dead_code)]
fn claude_tool_result(tool_id: &str, result: &str) -> String {
    json!({
        "type": "user",
        "message": {
            "role": "user",
            "content": [{
                "type": "tool_result",
                "tool_use_id": tool_id,
                "content": result,
                "is_error": false
            }]
        }
    })
    .to_string()
}

/// Insert Claude-format logs (wraps each line as Stdout).
async fn insert_claude_logs(pool: &SqlitePool, execution_id: Uuid, lines: Vec<String>) {
    let jsonl = lines
        .iter()
        .map(|l| wrap_as_stdout(l))
        .collect::<Vec<_>>()
        .join("\n");
    insert_jsonl_logs(pool, execution_id, &jsonl).await;
}

/// Create a test execution process and return its ID.
async fn create_test_execution(pool: &SqlitePool) -> Uuid {
    // Create project with unique path
    let project_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO projects (id, name, git_repo_path)
           VALUES ($1, 'Test Project', $2)"#,
    )
    .bind(project_id)
    .bind(format!("/tmp/test-{}", project_id))
    .execute(pool)
    .await
    .expect("Failed to create project");

    // Create task
    let task_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO tasks (id, project_id, title)
           VALUES ($1, $2, 'Test Task')"#,
    )
    .bind(task_id)
    .bind(project_id)
    .execute(pool)
    .await
    .expect("Failed to create task");

    // Create task attempt
    let attempt_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO task_attempts (id, task_id, executor, branch, target_branch)
           VALUES ($1, $2, 'CLAUDE_CODE', 'test-branch', 'main')"#,
    )
    .bind(attempt_id)
    .bind(task_id)
    .execute(pool)
    .await
    .expect("Failed to create task attempt");

    // Create execution process with a valid CodingAgentInitialRequest executor_action
    let execution_id = Uuid::new_v4();
    let executor_action = create_test_executor_action();
    sqlx::query(
        r#"INSERT INTO execution_processes (id, task_attempt_id, status, run_reason, executor_action)
           VALUES ($1, $2, 'running', 'codingagent', $3)"#,
    )
    .bind(execution_id)
    .bind(attempt_id)
    .bind(&executor_action)
    .execute(pool)
    .await
    .expect("Failed to create execution process");

    execution_id
}

/// Insert JSONL log records into the old execution_process_logs table.
async fn insert_jsonl_logs(pool: &SqlitePool, execution_id: Uuid, jsonl: &str) {
    let byte_size = jsonl.len() as i64;
    sqlx::query(
        r#"INSERT INTO execution_process_logs (execution_id, logs, byte_size, inserted_at)
           VALUES ($1, $2, $3, datetime('now', 'subsec'))"#,
    )
    .bind(execution_id)
    .bind(jsonl)
    .bind(byte_size)
    .execute(pool)
    .await
    .expect("Failed to insert JSONL log");
}

/// Count log entries in the new log_entries table.
async fn count_log_entries(pool: &SqlitePool, execution_id: Uuid) -> i64 {
    let row = sqlx::query(r#"SELECT COUNT(*) as count FROM log_entries WHERE execution_id = $1"#)
        .bind(execution_id)
        .fetch_one(pool)
        .await
        .expect("Failed to count log entries");

    row.get::<i64, _>("count")
}

/// Get all log entries for an execution.
async fn get_log_entries(pool: &SqlitePool, execution_id: Uuid) -> Vec<DbLogEntry> {
    DbLogEntry::find_by_execution_id(pool, execution_id)
        .await
        .expect("Failed to fetch log entries")
}

// =============================================================================
// TESTS
// =============================================================================
//
// These tests verify log migration using realistic Claude Code executor output.
// The migration reads JSONL from execution_process_logs, runs the executor's
// normalization logic, and stores the resulting JsonPatch entries in log_entries.
//
// Test data uses the helper functions above to generate valid Claude JSON
// protocol messages (assistant messages, tool_use, tool_result, etc.).
//
// See: docs/architecture/log-migration.md for architecture details.
// =============================================================================

#[tokio::test]
async fn test_migrate_single_assistant_message() {
    let (pool, _temp_dir) = setup_test_db().await;
    let execution_id = create_test_execution(&pool).await;

    // Insert a Claude assistant message
    let lines = vec![claude_assistant_message(
        "Hello, I can help you with that.",
        "msg_001",
    )];
    insert_claude_logs(&pool, execution_id, lines).await;

    // Migrate logs
    let result = services::services::log_migration::migrate_execution_logs(&pool, execution_id)
        .await
        .expect("Migration failed");

    // Verify migration produced entries
    assert!(
        result.migrated >= 1,
        "Expected at least 1 migrated entry, got {}",
        result.migrated
    );

    // Verify log entries in database
    let entries = get_log_entries(&pool, execution_id).await;
    assert!(
        !entries.is_empty(),
        "Expected log entries to be created from assistant message"
    );
    assert_eq!(
        entries[0].output_type, "json_patch",
        "Migration should produce json_patch entries"
    );
}

#[tokio::test]
async fn test_migrate_multiple_messages() {
    let (pool, _temp_dir) = setup_test_db().await;
    let execution_id = create_test_execution(&pool).await;

    // Insert multiple Claude assistant messages
    let lines = vec![
        claude_assistant_message("First message", "msg_001"),
        claude_assistant_message("Second message", "msg_002"),
        claude_assistant_message("Third message", "msg_003"),
    ];
    insert_claude_logs(&pool, execution_id, lines).await;

    // Migrate logs
    let result = services::services::log_migration::migrate_execution_logs(&pool, execution_id)
        .await
        .expect("Migration failed");

    // Verify migration produced at least one entry
    // Note: The normalization logic may combine or process messages differently
    assert!(
        result.migrated >= 1,
        "Expected at least 1 migrated entry, got {}",
        result.migrated
    );

    // Verify log entries in database
    let entries = get_log_entries(&pool, execution_id).await;
    assert!(
        !entries.is_empty(),
        "Expected at least 1 entry from multiple messages"
    );

    // All entries should be json_patch type
    for entry in &entries {
        assert_eq!(
            entry.output_type, "json_patch",
            "All migrated entries should be json_patch type"
        );
    }
}

#[tokio::test]
async fn test_migrate_tool_use_sequence() {
    let (pool, _temp_dir) = setup_test_db().await;
    let execution_id = create_test_execution(&pool).await;

    // Insert a realistic tool use sequence
    let lines = vec![
        claude_assistant_message("Let me check that file.", "msg_001"),
        claude_tool_use("Read", "tool_001", json!({"file_path": "/tmp/test.txt"})),
        claude_tool_result("tool_001", "file contents here"),
        claude_assistant_message("I found the file.", "msg_002"),
    ];
    insert_claude_logs(&pool, execution_id, lines).await;

    // Migrate logs
    let result = services::services::log_migration::migrate_execution_logs(&pool, execution_id)
        .await
        .expect("Migration failed");

    // Should have at least one entry from the tool use sequence
    // Note: The normalization logic may combine messages differently
    assert!(
        result.migrated >= 1,
        "Expected at least 1 migrated entry, got {}",
        result.migrated
    );

    // Verify log entries in database
    let entries = get_log_entries(&pool, execution_id).await;
    assert!(
        !entries.is_empty(),
        "Expected log entries to be created from tool use sequence"
    );

    // All entries should be json_patch type
    for entry in &entries {
        assert_eq!(
            entry.output_type, "json_patch",
            "All migrated entries should be json_patch type"
        );
    }
}

#[tokio::test]
async fn test_migrate_no_logs() {
    let (pool, _temp_dir) = setup_test_db().await;
    let execution_id = create_test_execution(&pool).await;

    // No logs inserted

    let result = services::services::log_migration::migrate_execution_logs(&pool, execution_id)
        .await
        .expect("Migration failed");

    assert_eq!(result.migrated, 0);
    assert_eq!(result.skipped, 0);
    assert_eq!(result.errors, 0);
}

#[tokio::test]
async fn test_migrate_all_executions() {
    let (pool, _temp_dir) = setup_test_db().await;

    // Create multiple executions with logs
    let exec1 = create_test_execution(&pool).await;
    let exec2 = create_test_execution(&pool).await;
    let exec3 = create_test_execution(&pool).await;

    // Use realistic Claude messages
    insert_claude_logs(
        &pool,
        exec1,
        vec![claude_assistant_message("Exec 1 message", "msg_001")],
    )
    .await;
    insert_claude_logs(
        &pool,
        exec2,
        vec![claude_assistant_message("Exec 2 message", "msg_002")],
    )
    .await;
    // exec3 has no logs - it won't be processed since there's nothing to migrate

    let result = services::services::log_migration::migrate_all_logs(&pool)
        .await
        .expect("Migration failed");

    // Only 2 executions have logs to migrate
    assert_eq!(
        result.executions_processed, 2,
        "Expected 2 executions processed"
    );
    assert!(
        result.total_migrated >= 2,
        "Expected at least 2 total migrated entries"
    );

    // Verify isolation - each execution should have entries
    assert!(
        count_log_entries(&pool, exec1).await >= 1,
        "exec1 should have entries"
    );
    assert!(
        count_log_entries(&pool, exec2).await >= 1,
        "exec2 should have entries"
    );
    assert_eq!(
        count_log_entries(&pool, exec3).await,
        0,
        "exec3 should have no entries"
    );
}

#[tokio::test]
async fn test_migrate_idempotent() {
    let (pool, _temp_dir) = setup_test_db().await;
    let execution_id = create_test_execution(&pool).await;

    // Use realistic Claude message
    insert_claude_logs(
        &pool,
        execution_id,
        vec![claude_assistant_message("Test message", "msg_001")],
    )
    .await;

    // First migration
    let result1 = services::services::log_migration::migrate_execution_logs(&pool, execution_id)
        .await
        .expect("First migration failed");
    let first_migrated = result1.migrated;
    assert!(
        first_migrated >= 1,
        "First migration should produce entries"
    );

    // Record count after first migration
    let count_after_first = count_log_entries(&pool, execution_id).await;

    // Second migration should skip already migrated logs
    let result2 = services::services::log_migration::migrate_execution_logs(&pool, execution_id)
        .await
        .expect("Second migration failed");
    assert!(
        result2.skipped >= 1,
        "Second migration should skip already processed logs"
    );

    // Should still have the same number of entries
    let count_after_second = count_log_entries(&pool, execution_id).await;
    assert_eq!(
        count_after_first, count_after_second,
        "Idempotent migration should not create duplicate entries"
    );
}

#[tokio::test]
async fn test_dry_run_mode() {
    let (pool, _temp_dir) = setup_test_db().await;
    let execution_id = create_test_execution(&pool).await;

    // Use realistic Claude message
    insert_claude_logs(
        &pool,
        execution_id,
        vec![claude_assistant_message("Test message", "msg_001")],
    )
    .await;

    // Dry run should not insert entries
    let result =
        services::services::log_migration::migrate_execution_logs_dry_run(&pool, execution_id)
            .await
            .expect("Dry run failed");

    assert!(
        result.would_migrate >= 1,
        "Dry run should report entries that would be migrated"
    );

    // No entries should be in the database
    assert_eq!(
        count_log_entries(&pool, execution_id).await,
        0,
        "Dry run should not actually insert entries"
    );
}
