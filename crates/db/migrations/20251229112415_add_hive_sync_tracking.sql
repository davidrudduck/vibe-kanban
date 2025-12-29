-- Add hive sync tracking fields to track which entities have been synced to the Hive.
-- These fields enable offline-first sync: entities created while offline can be
-- synced later when the node reconnects to the Hive.

-- Add hive_synced_at to task_attempts
-- NULL means not yet synced to Hive
ALTER TABLE task_attempts ADD COLUMN hive_synced_at TEXT;

-- Add hive_synced_at to execution_processes
-- NULL means not yet synced to Hive
ALTER TABLE execution_processes ADD COLUMN hive_synced_at TEXT;

-- Add hive_synced_at to log_entries
-- NULL means not yet synced to Hive
ALTER TABLE log_entries ADD COLUMN hive_synced_at TEXT;

-- Index for finding unsynced task_attempts efficiently
CREATE INDEX IF NOT EXISTS idx_task_attempts_hive_unsynced
    ON task_attempts(id) WHERE hive_synced_at IS NULL;

-- Index for finding unsynced execution_processes efficiently
CREATE INDEX IF NOT EXISTS idx_execution_processes_hive_unsynced
    ON execution_processes(id) WHERE hive_synced_at IS NULL;

-- Index for finding unsynced log_entries efficiently (by execution_id for batching)
CREATE INDEX IF NOT EXISTS idx_log_entries_hive_unsynced
    ON log_entries(execution_id) WHERE hive_synced_at IS NULL;
