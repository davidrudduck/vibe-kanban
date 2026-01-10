//! Tasks route module - HTTP handlers for task operations.

pub mod handlers;
pub mod types;

// Re-export types for public API
pub use types::{
    ArchiveTaskRequest, ArchiveTaskResponse, CreateAndStartTaskRequest, TaskQuery,
};

// Router will be implemented after all handlers are migrated
