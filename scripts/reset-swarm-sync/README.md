# Swarm Sync Reset Scripts

These scripts clear all hive sync state to allow a fresh re-synchronization.

## When to Use

- Duplicate `shared_tasks` causing mismatched data
- Tasks showing wrong attempts or missing labels
- Cross-node task viewing not working
- After schema changes that affect sync

## Execution Order

### 1. Stop all nodes first (recommended)
```bash
# On each node, stop the vibe-kanban server
cd /home/david/Code/vibe-kanban && pnpm run stop
```

### 2. Clear the Hive (run on TARDIS)
```bash
docker exec -i remote-remote-db-1 psql -U remote -d remote < 01-clear-hive.sql
```

### 3. Clear each node's sync links

**TARDIS:**
```bash
sqlite3 /home/david/.vkswarm/db/db.sqlite < 02-clear-node-links.sql
```

**TheDoctor:**
```bash
sqlite3 /home/david/.vkswarm/db/db.sqlite < 02-clear-node-links.sql
```

**justX:**
```bash
sqlite3 /home/david/.vkswarm/db/db.sqlite < 02-clear-node-links.sql
```

### 4. Restart all nodes
```bash
# On each node
cd /home/david/Code/vibe-kanban && pnpm run dev
```

### 5. Verify re-sync
The HiveSyncService will automatically:
1. Push local tasks to hive (creating new shared_tasks)
2. Push local task_attempts to hive
3. Sync labels

Check hive after a few minutes:
```bash
docker exec -i remote-remote-db-1 psql -U remote -d remote -c "SELECT COUNT(*) FROM shared_tasks;"
```

## What Gets Preserved

- All local tasks and their data
- All local task attempts and logs
- Project configurations
- User settings

## What Gets Cleared

- Hive: All shared_tasks, node_task_attempts, shared_task_labels
- Nodes: shared_task_id links (re-created on sync)
