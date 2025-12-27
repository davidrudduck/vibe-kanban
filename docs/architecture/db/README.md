# Database Documentation

Comprehensive documentation for Vibe Kanban's dual-database architecture.

## Architecture

| Document | Description |
|----------|-------------|
| [Overview](./database-overview.mdx) | Dual-database architecture (SQLite + PostgreSQL) |
| [Synchronization](./database-synchronization.mdx) | ElectricSQL, WebSocket, and REST sync |

## Schema Documentation

| Document | Description |
|----------|-------------|
| [SQLite Local Schema](./sqlite-local-schema.mdx) | 17 tables for local node storage |
| [PostgreSQL Hive Schema](./postgresql-hive-schema.mdx) | 18 tables for central hive |

## Function Reference

### SQLite (Local)

| Document | Model | Key Functions |
|----------|-------|---------------|
| [Project](./functions/sqlite-project.mdx) | `project.rs` | CRUD, remote sync, GitHub |
| [Task](./functions/sqlite-task.mdx) | `task.rs` | CRUD, archiving, sync |
| [Task Attempt](./functions/sqlite-task-attempt.mdx) | `task_attempt.rs` | Lifecycle, cleanup |
| [Execution Process](./functions/sqlite-execution-process.mdx) | `execution_process.rs` | Tracking, status |
| [Log Entry](./functions/sqlite-log-entry.mdx) | `log_entry.rs` | ElectricSQL logs |
| [Merge](./functions/sqlite-merge.mdx) | `merge.rs` | Direct/PR records |
| [Executor Session](./functions/sqlite-executor-session.mdx) | `executor_session.rs` | Agent sessions |
| [Supporting](./functions/sqlite-supporting.mdx) | Various | Templates, labels, etc. |

### PostgreSQL (Hive)

| Document | Repository | Key Functions |
|----------|------------|---------------|
| [Nodes](./functions/postgresql-nodes.mdx) | `nodes.rs` | Registration, heartbeat |
| [API Keys](./functions/postgresql-node-api-keys.mdx) | `node_api_keys.rs` | Key management |
| [Tasks](./functions/postgresql-tasks.mdx) | `tasks.rs` | Shared task CRUD |
| [Projects](./functions/postgresql-projects.mdx) | `projects.rs` | Hive projects |
| [Assignments](./functions/postgresql-assignments.mdx) | `task_assignments.rs` | Task dispatch |
| [Auth](./functions/postgresql-auth.mdx) | `auth.rs` | Sessions, OAuth |

## Quick Reference

**Local Node (SQLite)**: Stores projects, tasks, execution history, and logs for a single machine. Each node operates independently and can work offline.

**Central Hive (PostgreSQL)**: Manages organizations, users, shared tasks, and node coordination. Enables distributed task management across multiple machines.

**Synchronization**: Uses ElectricSQL for real-time data sync and WebSocket for task assignment and execution events.

## Related Documentation

- [Swarm/Hive Setup Guide](../../swarm-hive-setup.mdx) - How to configure nodes to connect to a hive
- [CLAUDE.md](/CLAUDE.md) - Development conventions and API patterns
