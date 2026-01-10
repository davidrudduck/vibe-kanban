//! Handler functions for tasks routes.
//!
//! Handlers are organized by concern:
//! - `core`: CRUD operations (create, read, update, delete, create-and-start)
//! - `status`: Archive, unarchive, assign, get children
//! - `labels`: Get labels, set labels
//! - `remote`: Remote/Hive task helpers (create, update, delete, resync)
//! - `streams`: WebSocket and streaming (task streams, available nodes, connection info)

pub mod core;
pub mod labels;
pub mod remote;
pub mod status;

// Re-export all handlers for convenient access from the router
pub use core::{
    create_task, create_task_and_start, delete_task, get_task, get_tasks, update_task,
};
pub use labels::{get_task_labels, set_task_labels};
pub use status::{archive_task, assign_task, get_task_children, unarchive_task};

// Note: remote helpers are pub(crate) and available via crate::routes::tasks_new::handlers::remote
