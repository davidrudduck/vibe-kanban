# Swarm Sync Reset Scripts

These scripts manage hive sync state for the swarm.

## When to Use

- **Dedupe**: Duplicate `shared_tasks` causing mismatched attempts/labels
- **Full Reset**: Complete re-sync needed after major issues or schema changes

## Quick Fix: Deduplicate (preserves data)

```bash
# 1. Stop all nodes
# 2. Dedupe hive (removes duplicates, keeps originals with attempts)
./run-reset.sh dedupe

# 3. Fix local links on each node
./run-reset.sh fix-links   # run on TARDIS, TheDoctor, justX

# 4. Restart all nodes with: pnpm run prod
```

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
# On each node (production mode)
cd /home/david/Code/vibe-kanban && pnpm run prod
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
