-- Reset hive_synced_at for attempts whose tasks have a shared_task_id.
-- This triggers re-sync so attempts are sent to hive with the correct shared_task_id.
-- Addresses issue where attempts were synced before tasks were linked to hive,
-- resulting in wrong shared_task_id values in hive's node_task_attempts table.

UPDATE task_attempts
SET hive_synced_at = NULL
WHERE task_id IN (
    SELECT id FROM tasks WHERE shared_task_id IS NOT NULL
);
