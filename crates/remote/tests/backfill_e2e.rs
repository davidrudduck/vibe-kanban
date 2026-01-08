//! End-to-end tests for backfill repository functionality.
//!
//! These tests verify the complete backfill flow from database state changes
//! through to the repository methods. They require a PostgreSQL database.
//!
//! # Prerequisites
//! - PostgreSQL database available at DATABASE_URL env var
//! - Migrations applied to the database
//!
//! # Running
//! ```bash
//! DATABASE_URL=postgres://... cargo test -p remote --test backfill_e2e
//! ```
//!
//! Note: Tests are skipped if DATABASE_URL is not set.

use chrono::Utc;
use sqlx::PgPool;
use uuid::Uuid;

use remote::db::node_task_attempts::{NodeTaskAttemptRepository, UpsertNodeTaskAttempt};

/// Helper to check if database is available.
fn database_url() -> Option<String> {
    std::env::var("DATABASE_URL").ok()
}

/// Skip test if database is not available.
macro_rules! skip_without_db {
    () => {
        if database_url().is_none() {
            eprintln!("Skipping test: DATABASE_URL not set");
            return;
        }
    };
}

/// Create a test database connection pool.
async fn create_pool() -> PgPool {
    let url = database_url().expect("DATABASE_URL must be set");
    PgPool::connect(&url)
        .await
        .expect("Failed to connect to database")
}

/// Create a test organization.
async fn create_test_organization(pool: &PgPool) -> Uuid {
    let org_id = Uuid::new_v4();
    let now = Utc::now();

    sqlx::query(
        r#"
        INSERT INTO organizations (id, name, created_at, updated_at)
        VALUES ($1, $2, $3, $4)
        "#,
    )
    .bind(org_id)
    .bind(format!("Test Org {}", org_id))
    .bind(now)
    .bind(now)
    .execute(pool)
    .await
    .expect("Failed to create test organization");

    org_id
}

/// Create a test node.
async fn create_test_node(pool: &PgPool, org_id: Uuid, is_online: bool) -> Uuid {
    let node_id = Uuid::new_v4();
    let now = Utc::now();
    // If online, last_seen is now; if offline, last_seen is 10 minutes ago
    let last_seen = if is_online {
        now
    } else {
        now - chrono::Duration::minutes(10)
    };

    sqlx::query(
        r#"
        INSERT INTO nodes (id, organization_id, machine_id, hostname, os_type, os_version, last_seen_at, created_at, updated_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        "#,
    )
    .bind(node_id)
    .bind(org_id)
    .bind(format!("machine-{}", node_id))
    .bind("test-host")
    .bind("linux")
    .bind("test")
    .bind(last_seen)
    .bind(now)
    .bind(now)
    .execute(pool)
    .await
    .expect("Failed to create test node");

    node_id
}

/// Create a test shared task.
async fn create_test_shared_task(pool: &PgPool, org_id: Uuid) -> Uuid {
    let task_id = Uuid::new_v4();
    let now = Utc::now();

    sqlx::query(
        r#"
        INSERT INTO shared_tasks (id, organization_id, title, status, created_at, updated_at)
        VALUES ($1, $2, $3, $4, $5, $6)
        "#,
    )
    .bind(task_id)
    .bind(org_id)
    .bind(format!("Test Task {}", task_id))
    .bind("todo")
    .bind(now)
    .bind(now)
    .execute(pool)
    .await
    .expect("Failed to create test shared task");

    task_id
}

/// Helper to create a node task attempt with specific sync_state.
async fn create_attempt_with_state(
    pool: &PgPool,
    node_id: Uuid,
    shared_task_id: Uuid,
    sync_state: &str,
) -> remote::nodes::NodeTaskAttempt {
    let repo = NodeTaskAttemptRepository::new(pool);
    let now = Utc::now();

    let data = UpsertNodeTaskAttempt {
        id: Uuid::new_v4(),
        assignment_id: Some(Uuid::new_v4()),
        shared_task_id,
        node_id,
        executor: "CLAUDE_CODE".to_string(),
        executor_variant: None,
        branch: "main".to_string(),
        target_branch: "main".to_string(),
        container_ref: None,
        worktree_deleted: false,
        setup_completed_at: None,
        created_at: now,
        updated_at: now,
    };

    let attempt = repo.upsert(&data).await.expect("Failed to create attempt");

    // Update sync_state if not 'partial' (default)
    if sync_state != "partial" {
        sqlx::query("UPDATE node_task_attempts SET sync_state = $1, sync_requested_at = NOW() WHERE id = $2")
            .bind(sync_state)
            .bind(attempt.id)
            .execute(pool)
            .await
            .expect("Failed to update sync_state");
    }

    // Re-fetch to get updated state
    repo.find_by_id(attempt.id)
        .await
        .expect("Failed to find attempt")
        .expect("Attempt not found")
}

/// Cleanup helper - removes test data created during a test.
async fn cleanup_attempt(pool: &PgPool, attempt_id: Uuid) {
    let _ = sqlx::query("DELETE FROM node_task_attempts WHERE id = $1")
        .bind(attempt_id)
        .execute(pool)
        .await;
}

async fn cleanup_node(pool: &PgPool, node_id: Uuid) {
    let _ = sqlx::query("DELETE FROM node_task_attempts WHERE node_id = $1")
        .bind(node_id)
        .execute(pool)
        .await;
    let _ = sqlx::query("DELETE FROM nodes WHERE id = $1")
        .bind(node_id)
        .execute(pool)
        .await;
}

async fn cleanup_task(pool: &PgPool, task_id: Uuid) {
    let _ = sqlx::query("DELETE FROM shared_tasks WHERE id = $1")
        .bind(task_id)
        .execute(pool)
        .await;
}

async fn cleanup_org(pool: &PgPool, org_id: Uuid) {
    let _ = sqlx::query("DELETE FROM organizations WHERE id = $1")
        .bind(org_id)
        .execute(pool)
        .await;
}

/// Test: find_incomplete_with_online_nodes returns only partial attempts from online nodes.
#[tokio::test]
async fn test_find_incomplete_with_online_nodes_basic() {
    skip_without_db!();

    let pool = create_pool().await;
    let org_id = create_test_organization(&pool).await;
    let online_node = create_test_node(&pool, org_id, true).await;
    let offline_node = create_test_node(&pool, org_id, false).await;
    let shared_task = create_test_shared_task(&pool, org_id).await;

    // Create attempts with different states
    let partial_online = create_attempt_with_state(&pool, online_node, shared_task, "partial").await;
    let complete_online = create_attempt_with_state(&pool, online_node, shared_task, "complete").await;
    let partial_offline = create_attempt_with_state(&pool, offline_node, shared_task, "partial").await;

    let repo = NodeTaskAttemptRepository::new(&pool);
    let incomplete = repo
        .find_incomplete_with_online_nodes(100, 0)
        .await
        .expect("Query failed");

    // Should only find the partial attempt from the online node
    let our_attempts: Vec<_> = incomplete
        .iter()
        .filter(|a| a.shared_task_id == shared_task)
        .collect();
    assert_eq!(our_attempts.len(), 1, "Should find exactly 1 incomplete attempt from online node");
    assert_eq!(our_attempts[0].node_id, online_node);
    assert_eq!(our_attempts[0].sync_state, "partial");

    // Cleanup
    cleanup_attempt(&pool, partial_online.id).await;
    cleanup_attempt(&pool, complete_online.id).await;
    cleanup_attempt(&pool, partial_offline.id).await;
    cleanup_node(&pool, online_node).await;
    cleanup_node(&pool, offline_node).await;
    cleanup_task(&pool, shared_task).await;
    cleanup_org(&pool, org_id).await;
}

/// Test: find_incomplete_with_online_nodes pagination works correctly.
#[tokio::test]
async fn test_find_incomplete_pagination() {
    skip_without_db!();

    let pool = create_pool().await;
    let org_id = create_test_organization(&pool).await;
    let online_node = create_test_node(&pool, org_id, true).await;
    let shared_task = create_test_shared_task(&pool, org_id).await;

    // Create 5 partial attempts
    let mut attempts = Vec::new();
    for _ in 0..5 {
        let attempt = create_attempt_with_state(&pool, online_node, shared_task, "partial").await;
        attempts.push(attempt);
    }

    let repo = NodeTaskAttemptRepository::new(&pool);

    // All 5 should be found with large limit
    let all = repo
        .find_incomplete_with_online_nodes(100, 0)
        .await
        .expect("Query failed");
    let our_attempts: Vec<_> = all
        .iter()
        .filter(|a| a.shared_task_id == shared_task)
        .collect();
    assert_eq!(our_attempts.len(), 5, "Should find all 5 incomplete attempts");

    // Cleanup
    for attempt in &attempts {
        cleanup_attempt(&pool, attempt.id).await;
    }
    cleanup_node(&pool, online_node).await;
    cleanup_task(&pool, shared_task).await;
    cleanup_org(&pool, org_id).await;
}

/// Test: mark_pending_backfill transitions partial -> pending_backfill.
#[tokio::test]
async fn test_mark_pending_backfill() {
    skip_without_db!();

    let pool = create_pool().await;
    let org_id = create_test_organization(&pool).await;
    let node_id = create_test_node(&pool, org_id, true).await;
    let shared_task = create_test_shared_task(&pool, org_id).await;

    let attempt = create_attempt_with_state(&pool, node_id, shared_task, "partial").await;

    let repo = NodeTaskAttemptRepository::new(&pool);
    let marked = repo
        .mark_pending_backfill(&[attempt.id])
        .await
        .expect("mark_pending_backfill failed");

    assert_eq!(marked, 1, "Should mark 1 attempt");

    // Verify state changed
    let updated = repo
        .find_by_id(attempt.id)
        .await
        .expect("find_by_id failed")
        .expect("Attempt not found");
    assert_eq!(updated.sync_state, "pending_backfill");
    assert!(updated.sync_requested_at.is_some());

    // Cleanup
    cleanup_attempt(&pool, attempt.id).await;
    cleanup_node(&pool, node_id).await;
    cleanup_task(&pool, shared_task).await;
    cleanup_org(&pool, org_id).await;
}

/// Test: mark_complete transitions to complete state.
#[tokio::test]
async fn test_mark_complete() {
    skip_without_db!();

    let pool = create_pool().await;
    let org_id = create_test_organization(&pool).await;
    let node_id = create_test_node(&pool, org_id, true).await;
    let shared_task = create_test_shared_task(&pool, org_id).await;

    let attempt = create_attempt_with_state(&pool, node_id, shared_task, "pending_backfill").await;

    let repo = NodeTaskAttemptRepository::new(&pool);
    let success = repo
        .mark_complete(attempt.id)
        .await
        .expect("mark_complete failed");

    assert!(success, "Should successfully mark as complete");

    // Verify state changed
    let updated = repo.find_by_id(attempt.id).await.unwrap().unwrap();
    assert_eq!(updated.sync_state, "complete");
    assert!(updated.last_full_sync_at.is_some());

    // Cleanup
    cleanup_attempt(&pool, attempt.id).await;
    cleanup_node(&pool, node_id).await;
    cleanup_task(&pool, shared_task).await;
    cleanup_org(&pool, org_id).await;
}

/// Test: reset_failed_backfill resets all pending_backfill for a specific node.
#[tokio::test]
async fn test_reset_failed_backfill() {
    skip_without_db!();

    let pool = create_pool().await;
    let org_id = create_test_organization(&pool).await;
    let node1 = create_test_node(&pool, org_id, true).await;
    let node2 = create_test_node(&pool, org_id, true).await;
    let shared_task = create_test_shared_task(&pool, org_id).await;

    // Create pending_backfill attempts on both nodes
    let attempt1 = create_attempt_with_state(&pool, node1, shared_task, "pending_backfill").await;
    let attempt2 = create_attempt_with_state(&pool, node1, shared_task, "pending_backfill").await;
    let attempt3 = create_attempt_with_state(&pool, node2, shared_task, "pending_backfill").await;

    let repo = NodeTaskAttemptRepository::new(&pool);

    // Reset only node1's failed backfills
    let reset = repo
        .reset_failed_backfill(node1)
        .await
        .expect("reset_failed_backfill failed");

    assert_eq!(reset, 2, "Should reset 2 attempts from node1");

    // Verify node1's attempts are reset to partial
    let a1 = repo.find_by_id(attempt1.id).await.unwrap().unwrap();
    let a2 = repo.find_by_id(attempt2.id).await.unwrap().unwrap();
    assert_eq!(a1.sync_state, "partial");
    assert_eq!(a2.sync_state, "partial");

    // Verify node2's attempt is still pending_backfill
    let a3 = repo.find_by_id(attempt3.id).await.unwrap().unwrap();
    assert_eq!(a3.sync_state, "pending_backfill");

    // Cleanup
    cleanup_attempt(&pool, attempt1.id).await;
    cleanup_attempt(&pool, attempt2.id).await;
    cleanup_attempt(&pool, attempt3.id).await;
    cleanup_node(&pool, node1).await;
    cleanup_node(&pool, node2).await;
    cleanup_task(&pool, shared_task).await;
    cleanup_org(&pool, org_id).await;
}

/// Test: Complete backfill flow - partial -> pending_backfill -> complete.
#[tokio::test]
async fn test_complete_backfill_flow() {
    skip_without_db!();

    let pool = create_pool().await;
    let org_id = create_test_organization(&pool).await;
    let node_id = create_test_node(&pool, org_id, true).await;
    let shared_task = create_test_shared_task(&pool, org_id).await;

    // 1. Create partial attempt
    let attempt = create_attempt_with_state(&pool, node_id, shared_task, "partial").await;
    assert_eq!(attempt.sync_state, "partial");

    let repo = NodeTaskAttemptRepository::new(&pool);

    // 2. Mark as pending_backfill (backfill request sent)
    let marked = repo.mark_pending_backfill(&[attempt.id]).await.unwrap();
    assert_eq!(marked, 1);

    let pending = repo.find_by_id(attempt.id).await.unwrap().unwrap();
    assert_eq!(pending.sync_state, "pending_backfill");
    assert!(pending.sync_requested_at.is_some());

    // 3. Mark as complete (backfill response received with success)
    let completed = repo.mark_complete(attempt.id).await.unwrap();
    assert!(completed);

    let complete = repo.find_by_id(attempt.id).await.unwrap().unwrap();
    assert_eq!(complete.sync_state, "complete");
    assert!(complete.last_full_sync_at.is_some());

    // 4. Verify it no longer appears in incomplete queries
    let incomplete = repo.find_incomplete_for_node(node_id).await.unwrap();
    let our_incomplete: Vec<_> = incomplete
        .iter()
        .filter(|a| a.id == attempt.id)
        .collect();
    assert!(our_incomplete.is_empty(), "Complete attempt should not appear in incomplete list");

    // Cleanup
    cleanup_attempt(&pool, attempt.id).await;
    cleanup_node(&pool, node_id).await;
    cleanup_task(&pool, shared_task).await;
    cleanup_org(&pool, org_id).await;
}

/// Test: Failed backfill flow - partial -> pending_backfill -> partial (retry).
#[tokio::test]
async fn test_failed_backfill_flow() {
    skip_without_db!();

    let pool = create_pool().await;
    let org_id = create_test_organization(&pool).await;
    let node_id = create_test_node(&pool, org_id, true).await;
    let shared_task = create_test_shared_task(&pool, org_id).await;

    // 1. Create partial attempt
    let attempt = create_attempt_with_state(&pool, node_id, shared_task, "partial").await;

    let repo = NodeTaskAttemptRepository::new(&pool);

    // 2. Mark as pending_backfill
    repo.mark_pending_backfill(&[attempt.id]).await.unwrap();

    // 3. Simulate backfill failure - reset to partial
    let reset = repo.reset_failed_backfill(node_id).await.unwrap();
    assert_eq!(reset, 1);

    let after_reset = repo.find_by_id(attempt.id).await.unwrap().unwrap();
    assert_eq!(after_reset.sync_state, "partial");

    // 4. Verify it still appears in incomplete queries (can be retried)
    let incomplete = repo.find_incomplete_for_node(node_id).await.unwrap();
    let our_incomplete: Vec<_> = incomplete
        .iter()
        .filter(|a| a.id == attempt.id)
        .collect();
    assert_eq!(our_incomplete.len(), 1);

    // Cleanup
    cleanup_attempt(&pool, attempt.id).await;
    cleanup_node(&pool, node_id).await;
    cleanup_task(&pool, shared_task).await;
    cleanup_org(&pool, org_id).await;
}
