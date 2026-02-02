-- NODE RESET SCRIPT
-- Run on each node's sqlite database:
--   TARDIS: sqlite3 /home/david/.vkswarm/db/db.sqlite < 02-clear-node-links.sql
--   TheDoctor: sqlite3 /home/david/.vkswarm/db/db.sqlite < 02-clear-node-links.sql
--   justX: sqlite3 /home/david/.vkswarm/db/db.sqlite < 02-clear-node-links.sql
--
-- This clears the hive sync state so tasks will re-sync on next cycle.
-- Local tasks and attempts are preserved - only the hive links are cleared.

-- Clear task sync state (tasks will re-push to hive)
-- Note: remote_version has NOT NULL DEFAULT 1, so reset to 1 not NULL
UPDATE tasks
SET shared_task_id = NULL,
    remote_version = 1,
    remote_last_synced_at = NULL
WHERE shared_task_id IS NOT NULL;

-- Clear attempt sync state (attempts will re-push to hive)
UPDATE task_attempts
SET hive_synced_at = NULL
WHERE hive_synced_at IS NOT NULL;

-- Clear activity cursors so we re-fetch from hive (table name is plural)
DELETE FROM shared_activity_cursors;

-- Report what was cleared
SELECT 'tasks_cleared' as metric, changes() as count;
