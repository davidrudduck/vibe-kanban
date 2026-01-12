//! Tasks route module - HTTP handlers for task operations.

use axum::{
    Router,
    middleware::from_fn_with_state,
    routing::{delete, get, post, put},
};

use crate::{DeploymentImpl, middleware::load_task_middleware};

pub mod handlers;
pub mod types;

// Re-export types for public API
pub use types::{
    ArchiveTaskRequest, ArchiveTaskResponse, CreateAndStartTaskRequest, TaskQuery,
    format_user_display_name,
};

/// Creates the tasks router with all task-related endpoints.
///
/// Routes are organized as:
/// - `GET /tasks` - List tasks for a project
/// - `POST /tasks` - Create a new task
/// - `GET /tasks/stream/ws` - WebSocket stream for task updates
/// - `POST /tasks/create-and-start` - Create and immediately start a task attempt
/// - `GET /tasks/{task_id}` - Get a specific task
/// - `PUT /tasks/{task_id}` - Update a task
/// - `DELETE /tasks/{task_id}` - Delete a task
/// - `POST /tasks/{task_id}/archive` - Archive a task
/// - `POST /tasks/{task_id}/unarchive` - Unarchive a task
/// - `POST /tasks/{task_id}/assign` - Assign a task
/// - `GET /tasks/{task_id}/children` - Get task children (subtasks)
/// - `GET /tasks/{task_id}/labels` - Get task labels
/// - `PUT /tasks/{task_id}/labels` - Set task labels
/// - `GET /tasks/{task_id}/available-nodes` - Get nodes where task's project exists
/// - `GET /tasks/{task_id}/stream-connection-info` - Get stream connection info for remote task
pub fn router(deployment: &DeploymentImpl) -> Router<DeploymentImpl> {
    // Routes that require task_id and load_task_middleware
    let task_actions_router = Router::new()
        .route("/", put(handlers::update_task))
        .route("/", delete(handlers::delete_task))
        .route("/archive", post(handlers::archive_task))
        .route("/unarchive", post(handlers::unarchive_task))
        .route("/assign", post(handlers::assign_task))
        .route("/children", get(handlers::get_task_children))
        .route(
            "/labels",
            get(handlers::get_task_labels).put(handlers::set_task_labels),
        )
        .route("/available-nodes", get(handlers::get_available_nodes))
        .route(
            "/stream-connection-info",
            get(handlers::get_stream_connection_info),
        );

    // Routes with {task_id} path parameter - apply load_task_middleware
    let task_id_router = Router::new()
        .route("/", get(handlers::get_task))
        .merge(task_actions_router)
        .layer(from_fn_with_state(deployment.clone(), load_task_middleware));

    // All task routes
    let inner = Router::new()
        .route("/", get(handlers::get_tasks).post(handlers::create_task))
        .route("/stream/ws", get(handlers::stream_tasks_ws))
        .route("/create-and-start", post(handlers::create_task_and_start))
        .nest("/{task_id}", task_id_router);

    // Mount under /tasks (will be nested under /projects/{project_id} by parent router)
    Router::new().nest("/tasks", inner)
}
