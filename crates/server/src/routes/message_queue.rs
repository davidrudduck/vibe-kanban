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

use crate::{DeploymentImpl, error::ApiError, middleware::load_task_attempt_middleware};

/// List all queued messages for a task attempt.
pub async fn list_queued_messages(
    Extension(task_attempt): Extension<TaskAttempt>,
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<Vec<QueuedMessage>>>, ApiError> {
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
    State(deployment): State<DeploymentImpl>,
    ResponseJson(payload): ResponseJson<AddQueuedMessageRequest>,
) -> Result<ResponseJson<ApiResponse<QueuedMessage>>, ApiError> {
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

/// Update an existing queued message.
pub async fn update_queued_message(
    Extension(task_attempt): Extension<TaskAttempt>,
    State(deployment): State<DeploymentImpl>,
    Path(message_id): Path<Uuid>,
    ResponseJson(payload): ResponseJson<UpdateQueuedMessageRequest>,
) -> Result<ResponseJson<ApiResponse<QueuedMessage>>, ApiError> {
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
            message_id,
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
pub async fn remove_queued_message(
    Extension(task_attempt): Extension<TaskAttempt>,
    State(deployment): State<DeploymentImpl>,
    Path(message_id): Path<Uuid>,
) -> Result<ResponseJson<ApiResponse<()>>, ApiError> {
    let removed = deployment
        .local_container()
        .message_queue()
        .remove(task_attempt.id, message_id)
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
    State(deployment): State<DeploymentImpl>,
    ResponseJson(payload): ResponseJson<ReorderQueuedMessagesRequest>,
) -> Result<ResponseJson<ApiResponse<Vec<QueuedMessage>>>, ApiError> {
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
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<()>>, ApiError> {
    deployment
        .local_container()
        .message_queue()
        .clear(task_attempt.id)
        .await;

    Ok(ResponseJson(ApiResponse::success(())))
}

pub fn router(deployment: &DeploymentImpl) -> Router<DeploymentImpl> {
    // Routes for individual messages (need message_id in path)
    let message_routes = Router::new()
        .route("/", put(update_queued_message))
        .route("/", delete(remove_queued_message));

    // Routes for the queue itself
    let queue_routes = Router::new()
        .route("/", get(list_queued_messages))
        .route("/", post(add_queued_message))
        .route("/", delete(clear_queued_messages))
        .route("/reorder", post(reorder_queued_messages))
        .nest("/{message_id}", message_routes)
        .layer(from_fn_with_state(
            deployment.clone(),
            load_task_attempt_middleware,
        ));

    Router::new().nest(
        "/task-attempts/{task_attempt_id}/message-queue",
        queue_routes,
    )
}
