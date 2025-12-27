# Architecture Documentation

This folder contains technical architecture documentation for Vibe Kanban's internal systems. These documents are intended for developers working on the codebase.

## Contents

| Document | Description |
|----------|-------------|
| [database-overview.mdx](./database-overview.mdx) | High-level overview of the dual-database architecture (SQLite + PostgreSQL) |
| [sqlite-local-schema.mdx](./sqlite-local-schema.mdx) | Complete SQLite schema for local node databases |
| [postgresql-hive-schema.mdx](./postgresql-hive-schema.mdx) | Complete PostgreSQL schema for the central hive server |
| [database-synchronization.mdx](./database-synchronization.mdx) | How data synchronizes between local nodes and the hive |

## Quick Reference

**Local Node (SQLite)**: Stores projects, tasks, execution history, and logs for a single machine. Each node operates independently and can work offline.

**Central Hive (PostgreSQL)**: Manages organizations, users, shared tasks, and node coordination. Enables distributed task management across multiple machines.

**Synchronization**: Uses ElectricSQL for real-time data sync and WebSocket for task assignment and execution events.

## Related Documentation

- [Swarm/Hive Setup Guide](../swarm-hive-setup.md) - How to configure nodes to connect to a hive
- [CLAUDE.md](/CLAUDE.md) - Development conventions and API patterns
