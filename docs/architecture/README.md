# Architecture Documentation

This folder contains technical architecture documentation for Vibe Kanban's internal systems. These documents are intended for developers working on the codebase.

## Contents

| Document | Description |
|----------|-------------|
| [Database Documentation](./db/) | Complete database architecture, schema, and function reference |

## Quick Reference

**Local Node (SQLite)**: Stores projects, tasks, execution history, and logs for a single machine. Each node operates independently and can work offline.

**Central Hive (PostgreSQL)**: Manages organizations, users, shared tasks, and node coordination. Enables distributed task management across multiple machines.

**Synchronization**: Uses ElectricSQL for real-time data sync and WebSocket for task assignment and execution events.

## Related Documentation

- [Swarm/Hive Setup Guide](../swarm-hive-setup.mdx) - How to configure nodes to connect to a hive
- [CLAUDE.md](/CLAUDE.md) - Development conventions and API patterns
