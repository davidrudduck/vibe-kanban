-- Drop the shared_tasks table
--
-- This table was used to cache tasks from the Hive locally. ElectricSQL now syncs
-- tasks directly to the tasks table via the shared_task_id foreign key.
--
-- The shared_activity_cursors table is kept as it tracks activity stream position
-- for label sync (which still uses the WebSocket activity stream).

-- Drop the shared_tasks table
DROP TABLE IF EXISTS shared_tasks;
