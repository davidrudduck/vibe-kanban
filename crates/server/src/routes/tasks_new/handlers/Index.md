# Task Handlers

Handler functions for task HTTP endpoints.

## Files
- `mod.rs` - Re-exports all handlers for convenient access
- `core.rs` - CRUD operations: create, read, update, delete, create-and-start
- `status.rs` - Status management: archive, unarchive, assign, get children
- `labels.rs` - Label operations: get labels, set labels
- `remote.rs` - Remote/Hive task helpers: create, update, delete, resync
- `streams.rs` - WebSocket and streaming: task streams, available nodes, connection info
