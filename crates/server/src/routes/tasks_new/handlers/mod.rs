//! Handler functions for tasks routes.
//!
//! Handlers are organized by concern:
//! - `core`: CRUD operations (create, read, update, delete, create-and-start)
//! - `status`: Archive, unarchive, assign, get children
//! - `labels`: Get labels, set labels
//! - `remote`: Remote/Hive task helpers (create, update, delete, resync)
//! - `streams`: WebSocket and streaming (task streams, available nodes, connection info)

// Handler modules will be added in subsequent sessions
