-- NODE LINK FIX SCRIPT
-- Run AFTER 03-dedupe-hive.sql on each node's sqlite:
--   sqlite3 /path/to/db.sqlite < 04-fix-node-links.sql
--
-- This clears shared_task_id for tasks that point to now-deleted duplicates.
-- The sync service will re-link them to the correct originals.

-- Clear shared_task_id for tasks whose link is now orphaned
-- (After hive dedupe, some shared_task_ids no longer exist)
-- Since we can't query hive from SQLite, we clear ALL links and let resync fix them
UPDATE tasks
SET shared_task_id = NULL,
    remote_version = 1,
    remote_last_synced_at = NULL
WHERE shared_task_id IS NOT NULL
  AND project_id IN (
    -- Only clear for swarm-linked projects
    SELECT id FROM projects WHERE remote_project_id IS NOT NULL
  );

-- Clear attempt sync state so they re-sync with correct shared_task_ids
UPDATE task_attempts
SET hive_synced_at = NULL
WHERE hive_synced_at IS NOT NULL
  AND task_id IN (
    SELECT id FROM tasks WHERE project_id IN (
      SELECT id FROM projects WHERE remote_project_id IS NOT NULL
    )
  );

-- Report
SELECT 'tasks_cleared' as metric, changes() as count;
