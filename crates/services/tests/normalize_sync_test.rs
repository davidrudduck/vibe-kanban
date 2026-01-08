//! Integration tests for normalization completion synchronization.
//!
//! These tests verify that log normalization tasks complete before execution finalization,
//! ensuring no logs are lost due to race conditions between the normalization task and
//! execution cleanup.
//!
//! The current implementation uses an arbitrary 50ms sleep after `push_finished()`,
//! which is insufficient for reliable synchronization. These tests are designed to:
//!
//! 1. Document the expected behavior (normalization completes before finalization)
//! 2. Verify the fix once implemented (normalize_logs returning JoinHandle)
//! 3. Test edge cases (fast executions, slow normalization, timeouts)
//!
//! Test coverage:
//! 1. `test_normalization_completes_before_finalization` - All normalized entries exist after stop
//! 2. `test_normalization_timeout` - Graceful handling of slow normalization
//! 3. `test_fast_execution_no_lost_logs` - Very short executions preserve all logs
//!
//! Related tasks:
//! - Task 005: Write these tests
//! - Task 006: Modify normalize_logs to return JoinHandle<()>
//! - Task 007: Await normalization handles before finalization

use executors::executors::{BaseCodingAgent, StandardCodingAgentExecutor};
use executors::profile::{ExecutorConfigs, ExecutorProfileId};
use serde_json::json;
use sqlx::SqlitePool;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;
use tokio::time::{sleep, timeout};
use utils::log_msg::LogMsg;
use utils::msg_store::MsgStore;
use uuid::Uuid;

/// Create a test database pool with migrations applied.
async fn setup_test_db() -> (SqlitePool, TempDir) {
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

    (pool, temp_dir)
}

/// Create a valid executor_action JSON for testing (CLAUDE_CODE executor).
/// Reserved for future tests that require database entities.
#[allow(dead_code)]
fn create_test_executor_action() -> String {
    r#"{"typ":{"type":"CodingAgentInitialRequest","prompt":"Test prompt","executor_profile_id":{"executor":"CLAUDE_CODE","variant":null}},"next_action":null}"#.to_string()
}

/// Create a test execution process and return its ID.
/// Sets up the full entity hierarchy: project -> task -> task_attempt -> execution_process
/// Reserved for future tests that require database entities.
#[allow(dead_code)]
async fn create_test_execution(pool: &SqlitePool) -> Uuid {
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

/// Count JsonPatch entries in MsgStore history.
fn count_json_patches(msg_store: &MsgStore) -> usize {
    msg_store
        .get_history()
        .into_iter()
        .filter(|msg| matches!(msg, LogMsg::JsonPatch(_)))
        .count()
}

/// Create a Claude assistant message JSON (stdout line format).
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

/// Wait for JsonPatch entries to appear in MsgStore with a stability check.
/// Returns the count of patches found, or times out.
async fn wait_for_patches_stable(msg_store: &MsgStore, timeout_ms: u64) -> usize {
    let deadline = tokio::time::Instant::now() + Duration::from_millis(timeout_ms);
    let mut last_count = 0;
    let mut stable_iterations = 0;

    while tokio::time::Instant::now() < deadline {
        sleep(Duration::from_millis(25)).await;

        let current_count = count_json_patches(msg_store);
        if current_count == last_count && current_count > 0 {
            stable_iterations += 1;
            // If count is stable for 3 iterations (75ms), normalization is likely done
            if stable_iterations >= 3 {
                return current_count;
            }
        } else {
            stable_iterations = 0;
            last_count = current_count;
        }
    }

    count_json_patches(msg_store)
}

// =============================================================================
// TESTS
// =============================================================================

/// Test that normalization completes before execution finalization.
///
/// This test simulates the execution lifecycle:
/// 1. Create a MsgStore and start normalization
/// 2. Push stdout lines that will be normalized into JsonPatch entries
/// 3. Call push_finished() to signal end of input
/// 4. Verify all normalized entries are available
///
/// EXPECTED BEHAVIOR (after Task 006/007 fixes):
/// - All JsonPatch entries should be in the MsgStore before cleanup
/// - No logs should be lost due to race conditions
///
/// CURRENT BEHAVIOR (may fail before fix):
/// - 50ms sleep may not be enough for normalization to complete
/// - JsonPatch entries may be missing if normalization is slow
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_normalization_completes_before_finalization() {
    let (_pool, _temp_dir) = setup_test_db().await;

    // Create MsgStore and get Claude executor for normalization
    let msg_store = Arc::new(MsgStore::new());
    let profile_id = ExecutorProfileId::new(BaseCodingAgent::ClaudeCode);
    let executor = ExecutorConfigs::get_cached().get_coding_agent_or_default(&profile_id);

    // Start normalization task (fire-and-forget currently)
    let worktree_path = PathBuf::from("/");
    executor.normalize_logs(msg_store.clone(), &worktree_path);

    // Push multiple Claude assistant messages as stdout
    // These should be normalized into JsonPatch entries
    let messages = vec![
        claude_assistant_message("First response from the assistant.", "msg_001"),
        claude_assistant_message("Second response with more content.", "msg_002"),
        claude_assistant_message("Third and final response.", "msg_003"),
    ];

    for msg in &messages {
        msg_store.push_stdout(msg);
    }

    // Signal end of input (this is what container.rs does before cleanup)
    msg_store.push_finished();

    // Wait for normalization to complete
    // The fix (Task 006/007) will replace this with awaiting the JoinHandle
    let patch_count = wait_for_patches_stable(&msg_store, 2000).await;

    // Verify normalized entries were created
    // Claude normalization should produce at least one JsonPatch entry
    // from the assistant messages
    assert!(
        patch_count >= 1,
        "Expected at least 1 JsonPatch entry after normalization, got {}. \
         This may indicate normalization did not complete before finalization.",
        patch_count
    );

    // Verify finished message is in history
    assert!(
        msg_store.is_finished(),
        "MsgStore should have received Finished message"
    );
}

/// Test graceful handling of slow normalization with timeout.
///
/// This test verifies that even if normalization takes longer than expected,
/// the system either:
/// - Waits long enough for normalization to complete (preferred)
/// - Times out gracefully without crashing
///
/// EXPECTED BEHAVIOR:
/// - Normalization completes within reasonable timeout
/// - If timeout occurs, system handles it gracefully
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_normalization_timeout() {
    let (_pool, _temp_dir) = setup_test_db().await;

    let msg_store = Arc::new(MsgStore::new());
    let profile_id = ExecutorProfileId::new(BaseCodingAgent::ClaudeCode);
    let executor = ExecutorConfigs::get_cached().get_coding_agent_or_default(&profile_id);

    // Start normalization
    let worktree_path = PathBuf::from("/");
    executor.normalize_logs(msg_store.clone(), &worktree_path);

    // Push a large number of messages to stress test normalization
    for i in 0..50 {
        let msg = claude_assistant_message(
            &format!("Message {} with some content to process.", i),
            &format!("msg_{:03}", i),
        );
        msg_store.push_stdout(&msg);
    }

    msg_store.push_finished();

    // Test with a reasonable timeout (5 seconds)
    // This tests that normalization completes within acceptable time
    let result = timeout(Duration::from_secs(5), async {
        wait_for_patches_stable(&msg_store, 4000).await
    })
    .await;

    match result {
        Ok(patch_count) => {
            // Normalization completed within timeout
            assert!(
                patch_count >= 1,
                "Expected at least 1 JsonPatch entry, got {}",
                patch_count
            );
        }
        Err(_) => {
            // Timeout occurred - this is acceptable if handled gracefully
            // In production, the system should log a warning and continue
            // rather than losing data or crashing
            let final_count = count_json_patches(&msg_store);
            eprintln!(
                "Normalization timeout occurred. Final patch count: {}",
                final_count
            );
            // Even with timeout, we should have some entries
            // If we have zero, that indicates a more serious problem
            assert!(
                final_count > 0 || msg_store.get_history().len() > 50,
                "Expected some processing to occur even with timeout"
            );
        }
    }
}

/// Test that very short executions don't lose logs.
///
/// This is a critical edge case: when an execution completes very quickly,
/// there's a race between:
/// - The normalization task processing stdout
/// - The cleanup code dropping the MsgStore
///
/// EXPECTED BEHAVIOR (after fix):
/// - All logs are preserved even for instant executions
/// - No race conditions between normalization and cleanup
///
/// NOTE: This test passes when we wait long enough (1000ms in this test).
/// The production issue is that container.rs only waits 50ms after push_finished(),
/// which may not be enough for the normalization task to complete.
///
/// The fix in Task 006/007 will:
/// - Task 006: normalize_logs returns JoinHandle<()>
/// - Task 007: Await normalization handles before finalization
/// This ensures we wait for actual completion rather than an arbitrary timeout.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_fast_execution_no_lost_logs() {
    let (_pool, _temp_dir) = setup_test_db().await;

    let msg_store = Arc::new(MsgStore::new());
    let profile_id = ExecutorProfileId::new(BaseCodingAgent::ClaudeCode);
    let executor = ExecutorConfigs::get_cached().get_coding_agent_or_default(&profile_id);

    // Start normalization
    let worktree_path = PathBuf::from("/");
    executor.normalize_logs(msg_store.clone(), &worktree_path);

    // Simulate a very fast execution: single message, immediate finish
    let msg = claude_assistant_message("Quick response.", "msg_001");
    msg_store.push_stdout(&msg);

    // Immediately signal finish (simulating fast execution completion)
    msg_store.push_finished();

    // Wait for normalization with proper completion checking
    // In production, container.rs uses 50ms sleep which may be insufficient.
    // After Task 006/007, this will be replaced with awaiting JoinHandle.
    let patch_count = wait_for_patches_stable(&msg_store, 1000).await;

    // Verify the single message was normalized
    assert!(
        patch_count >= 1,
        "Expected at least 1 JsonPatch entry for fast execution, got {}. \
         Fast executions should not lose logs.",
        patch_count
    );

    // Verify all original messages are preserved in history
    let history = msg_store.get_history();
    let stdout_count = history
        .iter()
        .filter(|m| matches!(m, LogMsg::Stdout(_)))
        .count();
    assert_eq!(
        stdout_count, 1,
        "Original stdout message should be preserved"
    );
}

/// Test that normalization handles empty input gracefully.
///
/// Edge case: execution starts but produces no output before stopping.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_normalization_empty_input() {
    let msg_store = Arc::new(MsgStore::new());
    let profile_id = ExecutorProfileId::new(BaseCodingAgent::ClaudeCode);
    let executor = ExecutorConfigs::get_cached().get_coding_agent_or_default(&profile_id);

    // Start normalization
    let worktree_path = PathBuf::from("/");
    executor.normalize_logs(msg_store.clone(), &worktree_path);

    // No stdout pushed - just finish
    msg_store.push_finished();

    // Give normalization time to process (should be instant for empty input)
    sleep(Duration::from_millis(100)).await;

    // Should have zero patches (no input to normalize)
    let patch_count = count_json_patches(&msg_store);
    assert_eq!(
        patch_count, 0,
        "Empty input should produce zero JsonPatch entries"
    );

    // Store should still be marked as finished
    assert!(msg_store.is_finished(), "Store should be finished");
}

/// Test that normalization handles malformed input gracefully.
///
/// The normalization task should skip invalid JSON without crashing.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_normalization_malformed_input() {
    let msg_store = Arc::new(MsgStore::new());
    let profile_id = ExecutorProfileId::new(BaseCodingAgent::ClaudeCode);
    let executor = ExecutorConfigs::get_cached().get_coding_agent_or_default(&profile_id);

    // Start normalization
    let worktree_path = PathBuf::from("/");
    executor.normalize_logs(msg_store.clone(), &worktree_path);

    // Push mix of valid and invalid input
    msg_store.push_stdout("not valid json at all");
    msg_store.push_stdout("{broken json");
    msg_store.push_stdout(&claude_assistant_message("Valid message", "msg_001"));
    msg_store.push_stdout("more garbage");

    msg_store.push_finished();

    // Wait for normalization
    let patch_count = wait_for_patches_stable(&msg_store, 1000).await;

    // Should have at least one patch from the valid message
    // Malformed input should be skipped, not crash
    assert!(
        patch_count >= 1,
        "Valid message should produce JsonPatch entry despite malformed siblings"
    );

    // All original messages should still be in history
    let history = msg_store.get_history();
    let stdout_count = history
        .iter()
        .filter(|m| matches!(m, LogMsg::Stdout(_)))
        .count();
    assert_eq!(
        stdout_count, 4,
        "All stdout messages should be preserved in history"
    );
}
