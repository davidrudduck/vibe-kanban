//! Integration tests for ElectricTaskSync service.
//!
//! Tests the integration of Electric Shape API with local SQLite task storage.
//! Uses a mock HTTP server to simulate Electric responses.

use db::models::task::Task;
use serde_json::json;
use services::services::electric_sync::{ElectricClient, ShapeConfig, ShapeOperation, ShapeState};
use sqlx::{
    SqlitePool,
    sqlite::{SqliteConnectOptions, SqliteJournalMode},
};
use std::str::FromStr;
use tempfile::TempDir;
use uuid::Uuid;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path_regex, query_param},
};

/// Set up an in-memory SQLite database with minimal test schema.
async fn setup_db() -> (SqlitePool, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    let options =
        SqliteConnectOptions::from_str(&format!("sqlite://{}", db_path.to_string_lossy()))
            .unwrap()
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal);

    let pool = SqlitePool::connect_with(options).await.unwrap();

    // Create minimal schema for testing Electric sync
    // Projects table
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS projects (
            id                       BLOB PRIMARY KEY,
            name                     TEXT NOT NULL,
            git_repo_path            TEXT,
            default_branch           TEXT,
            is_remote                INTEGER NOT NULL DEFAULT 0,
            remote_project_id        BLOB,
            remote_organization_id   BLOB,
            remote_name              TEXT,
            remote_last_synced_at    TEXT,
            created_at               TEXT NOT NULL DEFAULT (datetime('now', 'subsec')),
            updated_at               TEXT NOT NULL DEFAULT (datetime('now', 'subsec'))
        )
        "#,
    )
    .execute(&pool)
    .await
    .unwrap();

    // Tasks table with all columns needed for Electric sync
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS tasks (
            id                          BLOB PRIMARY KEY,
            project_id                  BLOB NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
            title                       TEXT NOT NULL,
            description                 TEXT,
            status                      TEXT NOT NULL DEFAULT 'todo'
                                        CHECK (status IN ('todo','inprogress','done','cancelled','inreview')),
            parent_task_id              BLOB REFERENCES tasks(id) ON DELETE SET NULL,
            shared_task_id              BLOB,
            remote_assignee_user_id     BLOB,
            remote_assignee_name        TEXT,
            remote_assignee_username    TEXT,
            remote_version              INTEGER NOT NULL DEFAULT 0,
            remote_last_synced_at       TEXT,
            remote_stream_node_id       BLOB,
            remote_stream_url           TEXT,
            archived_at                 TEXT,
            activity_at                 TEXT,
            created_at                  TEXT NOT NULL DEFAULT (datetime('now', 'subsec')),
            updated_at                  TEXT NOT NULL DEFAULT (datetime('now', 'subsec'))
        )
        "#,
    )
    .execute(&pool)
    .await
    .unwrap();

    // Index on shared_task_id for upsert operations
    sqlx::query(
        r#"
        CREATE UNIQUE INDEX IF NOT EXISTS idx_tasks_shared_task_unique
            ON tasks(shared_task_id)
            WHERE shared_task_id IS NOT NULL
        "#,
    )
    .execute(&pool)
    .await
    .unwrap();

    (pool, temp_dir)
}

/// Create a mock NDJSON response for Electric Shape API.
fn create_ndjson_response(operations: &[&str]) -> String {
    operations.join("\n")
}

/// Create a shared task insert operation.
fn create_task_insert(
    id: Uuid,
    project_id: Uuid,
    title: &str,
    status: &str,
    version: i64,
) -> String {
    let key = format!(r#""public"."shared_tasks"/"{id}""#);
    json!({
        "key": key,
        "value": {
            "id": id.to_string(),
            "project_id": project_id.to_string(),
            "title": title,
            "description": null,
            "status": status,
            "version": version,
            "assignee_user_id": null,
            "created_at": "2025-01-01T00:00:00Z",
            "updated_at": "2025-01-01T00:00:00Z"
        },
        "headers": {"operation": "insert"}
    })
    .to_string()
}

/// Create a shared task update operation.
fn create_task_update(
    id: Uuid,
    project_id: Uuid,
    title: &str,
    status: &str,
    version: i64,
) -> String {
    let key = format!(r#""public"."shared_tasks"/"{id}""#);
    json!({
        "key": key,
        "value": {
            "id": id.to_string(),
            "project_id": project_id.to_string(),
            "title": title,
            "description": null,
            "status": status,
            "version": version,
            "assignee_user_id": null,
            "created_at": "2025-01-01T00:00:00Z",
            "updated_at": "2025-01-01T01:00:00Z"
        },
        "headers": {"operation": "update"}
    })
    .to_string()
}

/// Create a shared task delete operation.
fn create_task_delete(id: Uuid) -> String {
    let key = format!(r#""public"."shared_tasks"/"{id}""#);
    json!({
        "key": key,
        "headers": {"operation": "delete"}
    })
    .to_string()
}

/// Create an up-to-date control message.
fn create_up_to_date() -> String {
    json!({"headers": {"control": "up-to-date"}}).to_string()
}

/// Create a must-refetch control message.
fn create_must_refetch() -> String {
    json!({"headers": {"control": "must-refetch"}}).to_string()
}

// ==================
// Shape Operation Parsing Tests
// ==================

#[test]
fn test_parse_task_insert_operation() {
    let task_id = Uuid::new_v4();
    let project_id = Uuid::new_v4();
    let json = create_task_insert(task_id, project_id, "Test Task", "todo", 1);

    let op = ShapeOperation::parse(&json).unwrap();

    match op {
        ShapeOperation::Insert { key, value } => {
            assert!(key.contains(&task_id.to_string()));
            assert_eq!(value["title"], "Test Task");
            assert_eq!(value["status"], "todo");
            assert_eq!(value["version"], 1);
        }
        _ => panic!("Expected Insert operation"),
    }
}

#[test]
fn test_parse_task_update_operation() {
    let task_id = Uuid::new_v4();
    let project_id = Uuid::new_v4();
    let json = create_task_update(task_id, project_id, "Updated Task", "inprogress", 2);

    let op = ShapeOperation::parse(&json).unwrap();

    match op {
        ShapeOperation::Update { key, value } => {
            assert!(key.contains(&task_id.to_string()));
            assert_eq!(value["title"], "Updated Task");
            assert_eq!(value["status"], "inprogress");
            assert_eq!(value["version"], 2);
        }
        _ => panic!("Expected Update operation"),
    }
}

#[test]
fn test_parse_task_delete_operation() {
    let task_id = Uuid::new_v4();
    let json = create_task_delete(task_id);

    let op = ShapeOperation::parse(&json).unwrap();

    match op {
        ShapeOperation::Delete { key } => {
            assert!(key.contains(&task_id.to_string()));
        }
        _ => panic!("Expected Delete operation"),
    }
}

// ==================
// Electric Client Tests with Mock Server
// ==================

#[tokio::test]
async fn test_electric_client_fetch_initial_sync() {
    let mock_server = MockServer::start().await;

    let task_id = Uuid::new_v4();
    let project_id = Uuid::new_v4();

    let ndjson = create_ndjson_response(&[
        &create_task_insert(task_id, project_id, "Task 1", "todo", 1),
        &create_up_to_date(),
    ]);

    Mock::given(method("GET"))
        .and(path_regex(r"/v1/shape.*"))
        .and(query_param("offset", "-1"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(ndjson)
                .append_header("electric-handle", "test-handle-123")
                .append_header("electric-offset", "100_0"),
        )
        .mount(&mock_server)
        .await;

    let config = ShapeConfig {
        base_url: mock_server.uri(),
        table: "shared_tasks".to_string(),
        where_clause: None,
        columns: None,
    };

    let client = ElectricClient::new(config).unwrap();
    let state = ShapeState::initial();

    let (operations, new_state) = client.fetch(&state, false).await.unwrap();

    assert_eq!(operations.len(), 2);
    assert!(matches!(&operations[0], ShapeOperation::Insert { .. }));
    assert!(matches!(&operations[1], ShapeOperation::UpToDate));
    assert_eq!(new_state.handle, Some("test-handle-123".to_string()));
    assert_eq!(new_state.offset, "100_0");
}

#[tokio::test]
async fn test_electric_client_fetch_with_handle() {
    let mock_server = MockServer::start().await;

    let task_id = Uuid::new_v4();
    let project_id = Uuid::new_v4();

    let ndjson = create_ndjson_response(&[
        &create_task_update(task_id, project_id, "Updated", "inprogress", 2),
        &create_up_to_date(),
    ]);

    Mock::given(method("GET"))
        .and(path_regex(r"/v1/shape.*"))
        .and(query_param("handle", "existing-handle"))
        .and(query_param("offset", "50_0"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(ndjson)
                .append_header("electric-handle", "existing-handle")
                .append_header("electric-offset", "100_0"),
        )
        .mount(&mock_server)
        .await;

    let config = ShapeConfig {
        base_url: mock_server.uri(),
        table: "shared_tasks".to_string(),
        where_clause: None,
        columns: None,
    };

    let client = ElectricClient::new(config).unwrap();
    let state = ShapeState {
        handle: Some("existing-handle".to_string()),
        offset: "50_0".to_string(),
    };

    let (operations, new_state) = client.fetch(&state, false).await.unwrap();

    assert_eq!(operations.len(), 2);
    assert!(matches!(&operations[0], ShapeOperation::Update { .. }));
    assert_eq!(new_state.offset, "100_0");
}

#[tokio::test]
async fn test_electric_client_must_refetch() {
    let mock_server = MockServer::start().await;

    let ndjson = create_ndjson_response(&[&create_must_refetch()]);

    Mock::given(method("GET"))
        .and(path_regex(r"/v1/shape.*"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(ndjson)
                .append_header("electric-offset", "-1"),
        )
        .mount(&mock_server)
        .await;

    let config = ShapeConfig {
        base_url: mock_server.uri(),
        table: "shared_tasks".to_string(),
        where_clause: None,
        columns: None,
    };

    let client = ElectricClient::new(config).unwrap();
    let state = ShapeState {
        handle: Some("old-handle".to_string()),
        offset: "50_0".to_string(),
    };

    let (operations, _new_state) = client.fetch(&state, false).await.unwrap();

    assert_eq!(operations.len(), 1);
    assert!(matches!(&operations[0], ShapeOperation::MustRefetch));
}

// ==================
// Task Upsert Integration Tests
// ==================

#[tokio::test]
async fn test_apply_insert_creates_task() {
    let (pool, _temp_dir) = setup_db().await;

    // Create a test project first
    let project_id = Uuid::new_v4();
    sqlx::query("INSERT INTO projects (id, name) VALUES (?, ?)")
        .bind(project_id)
        .bind("Test Project")
        .execute(&pool)
        .await
        .unwrap();

    let shared_task_id = Uuid::new_v4();

    // Apply insert operation
    Task::upsert_remote_task(
        &pool,
        Uuid::new_v4(),
        project_id,
        shared_task_id,
        "New Task".to_string(),
        None,
        db::models::task::TaskStatus::Todo,
        None,
        None,
        None,
        1,
        None,
        None,
    )
    .await
    .unwrap();

    // Verify task was created
    let task = Task::find_by_shared_task_id(&pool, shared_task_id)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(task.title, "New Task");
    assert_eq!(task.shared_task_id, Some(shared_task_id));
}

#[tokio::test]
async fn test_apply_update_modifies_task() {
    let (pool, _temp_dir) = setup_db().await;

    // Create a test project
    let project_id = Uuid::new_v4();
    sqlx::query("INSERT INTO projects (id, name) VALUES (?, ?)")
        .bind(project_id)
        .bind("Test Project")
        .execute(&pool)
        .await
        .unwrap();

    let shared_task_id = Uuid::new_v4();

    // Create initial task
    Task::upsert_remote_task(
        &pool,
        Uuid::new_v4(),
        project_id,
        shared_task_id,
        "Original".to_string(),
        None,
        db::models::task::TaskStatus::Todo,
        None,
        None,
        None,
        1,
        None,
        None,
    )
    .await
    .unwrap();

    // Apply update
    Task::upsert_remote_task(
        &pool,
        Uuid::new_v4(),
        project_id,
        shared_task_id,
        "Updated".to_string(),
        Some("Description".to_string()),
        db::models::task::TaskStatus::InProgress,
        None,
        None,
        None,
        2,
        None,
        None,
    )
    .await
    .unwrap();

    // Verify update applied
    let task = Task::find_by_shared_task_id(&pool, shared_task_id)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(task.title, "Updated");
    assert_eq!(task.description, Some("Description".to_string()));
    assert_eq!(task.status, db::models::task::TaskStatus::InProgress);
}

#[tokio::test]
async fn test_apply_delete_removes_task() {
    let (pool, _temp_dir) = setup_db().await;

    // Create a test project
    let project_id = Uuid::new_v4();
    sqlx::query("INSERT INTO projects (id, name) VALUES (?, ?)")
        .bind(project_id)
        .bind("Test Project")
        .execute(&pool)
        .await
        .unwrap();

    let shared_task_id = Uuid::new_v4();

    // Create task
    Task::upsert_remote_task(
        &pool,
        Uuid::new_v4(),
        project_id,
        shared_task_id,
        "To Delete".to_string(),
        None,
        db::models::task::TaskStatus::Todo,
        None,
        None,
        None,
        1,
        None,
        None,
    )
    .await
    .unwrap();

    // Delete via shared_task_id
    Task::delete_by_shared_task_id(&pool, shared_task_id)
        .await
        .unwrap();

    // Verify deleted
    let task = Task::find_by_shared_task_id(&pool, shared_task_id)
        .await
        .unwrap();

    assert!(task.is_none());
}

// ==================
// UUID Extraction Tests
// ==================

#[test]
fn test_extract_uuid_from_electric_key() {
    // Electric keys have the format: "schema"."table"/"uuid"
    let task_id = Uuid::new_v4();
    let key = format!(r#""public"."shared_tasks"/"{task_id}""#);

    // Extract UUID from key
    let extracted = extract_uuid_from_key(&key).unwrap();
    assert_eq!(extracted, task_id);
}

#[test]
fn test_extract_uuid_from_simple_key() {
    // Some Electric shapes use simple UUID keys
    let task_id = Uuid::new_v4();
    let key = task_id.to_string();

    let extracted = extract_uuid_from_key(&key).unwrap();
    assert_eq!(extracted, task_id);
}

/// Extract UUID from an Electric shape key.
/// Handles both simple UUID keys and complex keys like "schema"."table"/"uuid".
fn extract_uuid_from_key(key: &str) -> Option<Uuid> {
    // Try parsing as a simple UUID first
    if let Ok(uuid) = Uuid::parse_str(key) {
        return Some(uuid);
    }

    // Extract from quoted format: ..."uuid"
    key.rsplit('/')
        .next()
        .and_then(|s| s.trim_matches('"').parse().ok())
}

// ==================
// Full Sync Cycle Integration Test
// ==================

#[tokio::test]
async fn test_full_sync_cycle() {
    let mock_server = MockServer::start().await;
    let (pool, _temp_dir) = setup_db().await;

    // Create a test project
    let project_id = Uuid::new_v4();
    sqlx::query("INSERT INTO projects (id, name) VALUES (?, ?)")
        .bind(project_id)
        .bind("Test Project")
        .execute(&pool)
        .await
        .unwrap();

    let task1_id = Uuid::new_v4();
    let task2_id = Uuid::new_v4();

    // Initial sync with 2 tasks
    let ndjson_initial = create_ndjson_response(&[
        &create_task_insert(task1_id, project_id, "Task 1", "todo", 1),
        &create_task_insert(task2_id, project_id, "Task 2", "todo", 1),
        &create_up_to_date(),
    ]);

    Mock::given(method("GET"))
        .and(path_regex(r"/v1/shape.*"))
        .and(query_param("offset", "-1"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(ndjson_initial)
                .append_header("electric-handle", "handle-1")
                .append_header("electric-offset", "100_0"),
        )
        .mount(&mock_server)
        .await;

    let config = ShapeConfig {
        base_url: mock_server.uri(),
        table: "shared_tasks".to_string(),
        where_clause: None,
        columns: None,
    };

    let client = ElectricClient::new(config).unwrap();
    let mut state = ShapeState::initial();

    // Perform initial sync
    let (operations, new_state) = client.fetch(&state, false).await.unwrap();
    state = new_state;

    // Apply operations
    for op in operations {
        match op {
            ShapeOperation::Insert { value, .. } | ShapeOperation::Update { value, .. } => {
                let shared_id: Uuid = value["id"].as_str().unwrap().parse().unwrap();
                let title = value["title"].as_str().unwrap();
                let status = match value["status"].as_str().unwrap() {
                    "todo" => db::models::task::TaskStatus::Todo,
                    "inprogress" => db::models::task::TaskStatus::InProgress,
                    _ => db::models::task::TaskStatus::Todo,
                };
                let version = value["version"].as_i64().unwrap();

                Task::upsert_remote_task(
                    &pool,
                    Uuid::new_v4(),
                    project_id,
                    shared_id,
                    title.to_string(),
                    None,
                    status,
                    None,
                    None,
                    None,
                    version,
                    None,
                    None,
                )
                .await
                .unwrap();
            }
            ShapeOperation::Delete { key } => {
                if let Some(id) = extract_uuid_from_key(&key) {
                    Task::delete_by_shared_task_id(&pool, id).await.unwrap();
                }
            }
            ShapeOperation::UpToDate | ShapeOperation::MustRefetch => {}
        }
    }

    // Verify initial sync
    let task1 = Task::find_by_shared_task_id(&pool, task1_id)
        .await
        .unwrap()
        .unwrap();
    let task2 = Task::find_by_shared_task_id(&pool, task2_id)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(task1.title, "Task 1");
    assert_eq!(task2.title, "Task 2");

    // Set up incremental update: update task1, delete task2
    mock_server.reset().await;

    let ndjson_update = create_ndjson_response(&[
        &create_task_update(task1_id, project_id, "Task 1 Updated", "inprogress", 2),
        &create_task_delete(task2_id),
        &create_up_to_date(),
    ]);

    Mock::given(method("GET"))
        .and(path_regex(r"/v1/shape.*"))
        .and(query_param("handle", "handle-1"))
        .and(query_param("offset", "100_0"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(ndjson_update)
                .append_header("electric-handle", "handle-1")
                .append_header("electric-offset", "200_0"),
        )
        .mount(&mock_server)
        .await;

    // Perform incremental sync
    let (operations, _new_state) = client.fetch(&state, false).await.unwrap();

    // Apply operations
    for op in operations {
        match op {
            ShapeOperation::Insert { value, .. } | ShapeOperation::Update { value, .. } => {
                let shared_id: Uuid = value["id"].as_str().unwrap().parse().unwrap();
                let title = value["title"].as_str().unwrap();
                let status = match value["status"].as_str().unwrap() {
                    "todo" => db::models::task::TaskStatus::Todo,
                    "inprogress" => db::models::task::TaskStatus::InProgress,
                    _ => db::models::task::TaskStatus::Todo,
                };
                let version = value["version"].as_i64().unwrap();

                Task::upsert_remote_task(
                    &pool,
                    Uuid::new_v4(),
                    project_id,
                    shared_id,
                    title.to_string(),
                    None,
                    status,
                    None,
                    None,
                    None,
                    version,
                    None,
                    None,
                )
                .await
                .unwrap();
            }
            ShapeOperation::Delete { key } => {
                if let Some(id) = extract_uuid_from_key(&key) {
                    Task::delete_by_shared_task_id(&pool, id).await.unwrap();
                }
            }
            ShapeOperation::UpToDate | ShapeOperation::MustRefetch => {}
        }
    }

    // Verify incremental update
    let task1 = Task::find_by_shared_task_id(&pool, task1_id)
        .await
        .unwrap()
        .unwrap();
    let task2 = Task::find_by_shared_task_id(&pool, task2_id).await.unwrap();

    assert_eq!(task1.title, "Task 1 Updated");
    assert_eq!(task1.status, db::models::task::TaskStatus::InProgress);
    assert!(task2.is_none(), "Task 2 should be deleted");
}
