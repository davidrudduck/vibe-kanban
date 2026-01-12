//! HTTP endpoints for the in-memory message queue.
//!
//! Messages can be queued for a task attempt and will be automatically sent
//! as follow-up requests when the current agent execution completes.

use axum::{
    Extension, Router,
    extract::{Path, State},
    middleware::from_fn_with_state,
    response::Json as ResponseJson,
    routing::{delete, get, post, put},
};
use db::models::task_attempt::TaskAttempt;
use local_deployment::message_queue::{
    AddQueuedMessageRequest, QueuedMessage, ReorderQueuedMessagesRequest,
    UpdateQueuedMessageRequest,
};
use utils::response::ApiResponse;
use uuid::Uuid;

use crate::{
    DeploymentImpl, error::ApiError,
    middleware::{RemoteTaskAttemptContext, load_task_attempt_middleware},
};
use deployment::Deployment;

/// List all queued messages for a task attempt.
pub async fn list_queued_messages(
    Extension(task_attempt): Extension<TaskAttempt>,
    remote_ctx: Option<Extension<RemoteTaskAttemptContext>>,
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<Vec<QueuedMessage>>>, ApiError> {
    // Message queue is local-only - remote tasks have no local queue
    if remote_ctx.is_some() {
        return Ok(ResponseJson(ApiResponse::success(vec![])));
    }

    let messages = deployment
        .local_container()
        .message_queue()
        .list(task_attempt.id)
        .await;

    Ok(ResponseJson(ApiResponse::success(messages)))
}

/// Maximum number of messages allowed in a queue.
const MAX_QUEUE_SIZE: usize = 50;

/// Add a new message to the queue.
pub async fn add_queued_message(
    Extension(task_attempt): Extension<TaskAttempt>,
    remote_ctx: Option<Extension<RemoteTaskAttemptContext>>,
    State(deployment): State<DeploymentImpl>,
    ResponseJson(payload): ResponseJson<AddQueuedMessageRequest>,
) -> Result<ResponseJson<ApiResponse<QueuedMessage>>, ApiError> {
    // Cannot modify message queue for remote task attempts
    if remote_ctx.is_some() {
        return Err(ApiError::BadRequest(
            "Cannot modify message queue for remote task attempts".into(),
        ));
    }

    if payload.content.trim().is_empty() {
        return Err(ApiError::BadRequest(
            "Message content cannot be empty".into(),
        ));
    }

    // Check queue limit
    let current_queue = deployment
        .local_container()
        .message_queue()
        .list(task_attempt.id)
        .await;
    if current_queue.len() >= MAX_QUEUE_SIZE {
        return Err(ApiError::BadRequest(format!(
            "Queue limit reached (max {} messages)",
            MAX_QUEUE_SIZE
        )));
    }

    let message = deployment
        .local_container()
        .message_queue()
        .add(task_attempt.id, payload.content, payload.variant)
        .await;

    Ok(ResponseJson(ApiResponse::success(message)))
}

/// Path parameters for routes with both task_attempt_id and message_id.
/// Using a struct allows Axum to extract named path parameters that are
/// separated by literal segments (like "message-queue").
#[derive(serde::Deserialize)]
pub struct MessageQueueParams {
    task_attempt_id: Uuid,
    message_id: Uuid,
}

/// Update an existing queued message.
/// This handler loads the TaskAttempt directly instead of using middleware,
/// because Axum's Path<(Uuid, Uuid)> tuple extraction requires consecutive
/// parameters, but this route has a literal segment between the UUIDs.
pub async fn update_queued_message(
    State(deployment): State<DeploymentImpl>,
    Path(params): Path<MessageQueueParams>,
    ResponseJson(payload): ResponseJson<UpdateQueuedMessageRequest>,
) -> Result<ResponseJson<ApiResponse<QueuedMessage>>, ApiError> {
    // Load task attempt directly (not using middleware due to path param limitations)
    let task_attempt = TaskAttempt::find_by_id(&deployment.db().pool, params.task_attempt_id)
        .await?
        .ok_or_else(|| ApiError::NotFound("Task attempt not found".into()))?;

    // Validate content if provided
    if let Some(ref content) = payload.content
        && content.trim().is_empty()
    {
        return Err(ApiError::BadRequest(
            "Message content cannot be empty".into(),
        ));
    }

    let message = deployment
        .local_container()
        .message_queue()
        .update(
            task_attempt.id,
            params.message_id,
            payload.content,
            payload.variant,
        )
        .await;

    match message {
        Some(msg) => Ok(ResponseJson(ApiResponse::success(msg))),
        None => Err(ApiError::NotFound("Message not found".into())),
    }
}

/// Remove a queued message.
/// This handler loads the TaskAttempt directly instead of using middleware,
/// because Axum's Path<(Uuid, Uuid)> tuple extraction requires consecutive
/// parameters, but this route has a literal segment between the UUIDs.
pub async fn remove_queued_message(
    State(deployment): State<DeploymentImpl>,
    Path(params): Path<MessageQueueParams>,
) -> Result<ResponseJson<ApiResponse<()>>, ApiError> {
    // Load task attempt directly (not using middleware due to path param limitations)
    let task_attempt = TaskAttempt::find_by_id(&deployment.db().pool, params.task_attempt_id)
        .await?
        .ok_or_else(|| ApiError::NotFound("Task attempt not found".into()))?;

    let removed = deployment
        .local_container()
        .message_queue()
        .remove(task_attempt.id, params.message_id)
        .await;

    if removed {
        Ok(ResponseJson(ApiResponse::success(())))
    } else {
        Err(ApiError::NotFound("Message not found".into()))
    }
}

/// Reorder queued messages.
pub async fn reorder_queued_messages(
    Extension(task_attempt): Extension<TaskAttempt>,
    remote_ctx: Option<Extension<RemoteTaskAttemptContext>>,
    State(deployment): State<DeploymentImpl>,
    ResponseJson(payload): ResponseJson<ReorderQueuedMessagesRequest>,
) -> Result<ResponseJson<ApiResponse<Vec<QueuedMessage>>>, ApiError> {
    // Cannot modify message queue for remote task attempts
    if remote_ctx.is_some() {
        return Err(ApiError::BadRequest(
            "Cannot modify message queue for remote task attempts".into(),
        ));
    }

    let result = deployment
        .local_container()
        .message_queue()
        .reorder(task_attempt.id, payload.message_ids)
        .await;

    match result {
        Some(messages) => Ok(ResponseJson(ApiResponse::success(messages))),
        None => Err(ApiError::BadRequest(
            "Invalid reorder: message IDs must match the current queue".into(),
        )),
    }
}

/// Clear all queued messages for a task attempt.
pub async fn clear_queued_messages(
    Extension(task_attempt): Extension<TaskAttempt>,
    remote_ctx: Option<Extension<RemoteTaskAttemptContext>>,
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<()>>, ApiError> {
    // Cannot modify message queue for remote task attempts
    if remote_ctx.is_some() {
        return Err(ApiError::BadRequest(
            "Cannot modify message queue for remote task attempts".into(),
        ));
    }

    deployment
        .local_container()
        .message_queue()
        .clear(task_attempt.id)
        .await;

    Ok(ResponseJson(ApiResponse::success(())))
}

pub fn router(deployment: &DeploymentImpl) -> Router<DeploymentImpl> {
    // Base routes (without {message_id}) use middleware to load TaskAttempt via Extension
    let base_routes = Router::new()
        .route("/", get(list_queued_messages))
        .route("/", post(add_queued_message))
        .route("/", delete(clear_queued_messages))
        .route("/reorder", post(reorder_queued_messages))
        .layer(from_fn_with_state(
            deployment.clone(),
            load_task_attempt_middleware,
        ));

    // Routes with {message_id} do NOT use middleware - they extract both path params
    // using a struct (MessageQueueParams) and load TaskAttempt directly in the handler.
    // This avoids Axum's limitation where Path<(Uuid, Uuid)> requires consecutive params.
    let message_routes = Router::new().route(
        "/task-attempts/{task_attempt_id}/message-queue/{message_id}",
        put(update_queued_message).delete(remove_queued_message),
    );

    Router::new()
        .nest(
            "/task-attempts/{task_attempt_id}/message-queue",
            base_routes,
        )
        .merge(message_routes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ApiError;
    use db::test_utils::create_test_pool;
    use uuid::Uuid;

    /// Test that reject_if_remote returns BadRequest for remote projects.
    /// This test verifies that when a task attempt belongs to a remote project
    /// (is_remote = true, source_node_id set), the function correctly rejects
    /// the operation with a BadRequest error.
    #[tokio::test]
    async fn test_reject_if_remote_rejects_remote_project() {
        let (pool, _temp_dir) = create_test_pool().await;

        // Create a remote project (is_remote = true, source_node_id set)
        let project_id = Uuid::new_v4();
        let source_node_id = Uuid::new_v4();
        sqlx::query(
            r#"INSERT INTO projects (id, name, git_repo_path, is_remote, source_node_id)
               VALUES ($1, $2, $3, $4, $5)"#,
        )
        .bind(project_id)
        .bind("Remote Test Project")
        .bind("/tmp/test-remote")
        .bind(true)
        .bind(source_node_id)
        .execute(&pool)
        .await
        .expect("Failed to create remote project");

        // Create a task associated with the remote project
        let task_id = Uuid::new_v4();
        sqlx::query(
            r#"INSERT INTO tasks (id, project_id, title, status)
               VALUES ($1, $2, $3, $4)"#,
        )
        .bind(task_id)
        .bind(project_id)
        .bind("Test Task")
        .bind("todo")
        .execute(&pool)
        .await
        .expect("Failed to create task");

        // Create a task attempt
        let attempt_id = Uuid::new_v4();
        sqlx::query(
            r#"INSERT INTO task_attempts (id, task_id, executor, branch, target_branch)
               VALUES ($1, $2, $3, $4, $5)"#,
        )
        .bind(attempt_id)
        .bind(task_id)
        .bind("CLAUDE_CODE")
        .bind("test-branch")
        .bind("main")
        .execute(&pool)
        .await
        .expect("Failed to create task attempt");

        // Load the task attempt
        let task_attempt = TaskAttempt::find_by_id(&pool, attempt_id)
            .await
            .expect("Failed to query task attempt")
            .expect("Task attempt not found");

        // Call reject_if_remote - should return BadRequest for remote project
        let result = reject_if_remote(&pool, &task_attempt).await;

        // Verify that the result is Err(ApiError::BadRequest(_))
        assert!(
            matches!(result, Err(ApiError::BadRequest(ref msg)) if msg.contains("remote")),
            "Expected BadRequest error for remote project, got: {:?}",
            result
        );
    }

    /// Test that reject_if_remote returns Ok(()) for local projects.
    /// This test verifies that when a task attempt belongs to a local project
    /// (is_remote = false, no source_node_id), the function correctly allows
    /// the operation to proceed by returning Ok(()).
    #[tokio::test]
    async fn test_reject_if_remote_allows_local_project() {
        let (pool, _temp_dir) = create_test_pool().await;

        // Create a local project (is_remote = false, no source_node_id)
        let project_id = Uuid::new_v4();
        sqlx::query(
            r#"INSERT INTO projects (id, name, git_repo_path, is_remote)
               VALUES ($1, $2, $3, $4)"#,
        )
        .bind(project_id)
        .bind("Local Test Project")
        .bind("/tmp/test-local")
        .bind(false)
        .execute(&pool)
        .await
        .expect("Failed to create local project");

        // Create a task associated with the local project
        let task_id = Uuid::new_v4();
        sqlx::query(
            r#"INSERT INTO tasks (id, project_id, title, status)
               VALUES ($1, $2, $3, $4)"#,
        )
        .bind(task_id)
        .bind(project_id)
        .bind("Test Task")
        .bind("todo")
        .execute(&pool)
        .await
        .expect("Failed to create task");

        // Create a task attempt
        let attempt_id = Uuid::new_v4();
        sqlx::query(
            r#"INSERT INTO task_attempts (id, task_id, executor, branch, target_branch)
               VALUES ($1, $2, $3, $4, $5)"#,
        )
        .bind(attempt_id)
        .bind(task_id)
        .bind("CLAUDE_CODE")
        .bind("test-branch")
        .bind("main")
        .execute(&pool)
        .await
        .expect("Failed to create task attempt");

        // Load the task attempt
        let task_attempt = TaskAttempt::find_by_id(&pool, attempt_id)
            .await
            .expect("Failed to query task attempt")
            .expect("Task attempt not found");

        // Call reject_if_remote - should return Ok(()) for local project
        let result = reject_if_remote(&pool, &task_attempt).await;

        // Verify that the result is Ok(())
        assert!(
            result.is_ok(),
            "Expected Ok(()) for local project, got: {:?}",
            result
        );
    }
}
