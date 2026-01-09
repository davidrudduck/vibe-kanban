//! Integration tests for LogBatcher finish signal behavior.
//!
//! These tests verify that calling `log_batcher.finish(execution_id)` properly
//! flushes all buffered logs to the database, even when the batch size threshold
//! hasn't been reached.
//!
//! Test coverage:
//! 1. `test_finish_flushes_remaining_logs` - Verifies finish() flushes buffered logs
//! 2. `test_finish_idempotent` - Calling finish() twice doesn't duplicate logs
//! 3. `test_finish_no_pending` - finish() on empty buffer is safe

use db::models::execution_process_logs::ExecutionProcessLogs;
use db::{DBService, DbMetrics};
use services::services::log_batcher::LogBatcher;
use sqlx::SqlitePool;
use std::time::Duration;
use tempfile::TempDir;
use tokio::time::sleep;
use utils::log_msg::LogMsg;
use uuid::Uuid;

/// Create a test database pool with migrations applied.
async fn setup_test_db() -> (DBService, TempDir) {
    use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
    use std::str::FromStr;

    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");

    let options =
        SqliteConnectOptions::from_str(&format!("sqlite://{}", db_path.to_string_lossy()))
            .expect("Invalid database URL")
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal);

    let pool = SqlitePoolOptions::new()
        .min_connections(1)
        .max_connections(5)
        .connect_with(options)
        .await
        .expect("Failed to create pool");

    // Run migrations
    sqlx::migrate!("../db/migrations")
        .run(&pool)
        .await
        .expect("Failed to run migrations");

    let db = DBService {
        pool,
        metrics: DbMetrics::new(),
    };

    (db, temp_dir)
}

/// Create a valid executor_action JSON for testing.
fn create_test_executor_action() -> String {
    r#"{"typ":{"type":"CodingAgentInitialRequest","prompt":"Test prompt","executor_profile_id":{"executor":"CLAUDE_CODE","variant":null}},"next_action":null}"#.to_string()
}

/// Create a test execution process and return its ID.
/// This sets up the full entity hierarchy: project -> task -> task_attempt -> execution_process
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

/// Count all JSONL log lines stored for an execution.
async fn count_log_lines(db: &DBService, execution_id: Uuid) -> usize {
    let records = ExecutionProcessLogs::find_by_execution_id(&db.pool, execution_id)
        .await
        .expect("Failed to fetch logs");

    // Count non-empty lines across all records
    records
        .iter()
        .flat_map(|r| r.logs.lines())
        .filter(|l| !l.trim().is_empty())
        .count()
}

/// Get all log content for an execution as a single string.
async fn get_log_content(db: &DBService, execution_id: Uuid) -> String {
    let records = ExecutionProcessLogs::find_by_execution_id(&db.pool, execution_id)
        .await
        .expect("Failed to fetch logs");

    records.iter().map(|r| r.logs.as_str()).collect()
}

/// Test that finish() flushes all remaining buffered logs to the database,
/// even when the batch size threshold hasn't been reached.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_finish_flushes_remaining_logs() {
    let (db, _temp_dir) = setup_test_db().await;
    let exec_id = create_test_execution(&db.pool).await;
    let handle = LogBatcher::spawn(&db);

    // Add 10 logs - well below the batch size of 100
    for i in 0..10 {
        handle
            .add_log(exec_id, LogMsg::Stdout(format!("line {}", i)))
            .await;
    }

    // Without finish(), logs may still be in buffer
    // Call finish() to flush remaining logs
    handle.finish(exec_id).await;

    // Wait briefly for async flush to complete
    sleep(Duration::from_millis(100)).await;

    // Verify all 10 logs are in the database
    let line_count = count_log_lines(&db, exec_id).await;
    assert!(
        line_count >= 10,
        "Expected at least 10 log lines after finish(), got {}",
        line_count
    );

    // Verify content contains expected messages
    let content = get_log_content(&db, exec_id).await;
    for i in 0..10 {
        assert!(
            content.contains(&format!("line {}", i)),
            "Missing log line {}",
            i
        );
    }

    // Clean shutdown
    handle.shutdown().await;
}

/// Test that calling finish() twice doesn't duplicate logs.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_finish_idempotent() {
    let (db, _temp_dir) = setup_test_db().await;
    let exec_id = create_test_execution(&db.pool).await;
    let handle = LogBatcher::spawn(&db);

    // Add 5 logs
    for i in 0..5 {
        handle
            .add_log(exec_id, LogMsg::Stdout(format!("message {}", i)))
            .await;
    }

    // Call finish() first time
    handle.finish(exec_id).await;
    sleep(Duration::from_millis(100)).await;

    // Count after first finish
    let count_after_first = count_log_lines(&db, exec_id).await;
    assert!(
        count_after_first >= 5,
        "Expected at least 5 log lines after first finish(), got {}",
        count_after_first
    );

    // Call finish() second time (should be idempotent)
    handle.finish(exec_id).await;
    sleep(Duration::from_millis(100)).await;

    // Count after second finish should be the same
    let count_after_second = count_log_lines(&db, exec_id).await;
    assert_eq!(
        count_after_first, count_after_second,
        "finish() should be idempotent - count changed from {} to {}",
        count_after_first, count_after_second
    );

    // Clean shutdown
    handle.shutdown().await;
}

/// Test that finish() on an execution with no pending logs is safe.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_finish_no_pending() {
    let (db, _temp_dir) = setup_test_db().await;
    let exec_id = create_test_execution(&db.pool).await;
    let handle = LogBatcher::spawn(&db);

    // Don't add any logs - just call finish on empty buffer (should not crash)
    handle.finish(exec_id).await;

    // Yield to allow batcher to process
    tokio::task::yield_now().await;
    sleep(Duration::from_millis(50)).await;

    // Should have no logs (not crash)
    let line_count = count_log_lines(&db, exec_id).await;
    assert_eq!(
        line_count, 0,
        "Expected 0 log lines for empty execution, got {}",
        line_count
    );

    // Should still be able to add logs after finish
    handle
        .add_log(exec_id, LogMsg::Stdout("after finish".to_string()))
        .await;

    // Use shutdown() which forces flush of all remaining logs
    handle.shutdown().await;

    // Now should have 1 log
    let line_count = count_log_lines(&db, exec_id).await;
    assert_eq!(
        line_count, 1,
        "Expected 1 log line after adding post-finish, got {}",
        line_count
    );
}
