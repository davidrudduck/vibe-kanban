-- Add activity_at column to shared_tasks table
-- This stores the timestamp from the remote activity event when a task's status changes

ALTER TABLE shared_tasks ADD COLUMN activity_at DATETIME DEFAULT NULL;

-- Backfill: use updated_at as a reasonable default for existing shared tasks
UPDATE shared_tasks SET activity_at = updated_at WHERE activity_at IS NULL;

-- Index for efficient ordering by activity timestamp
CREATE INDEX idx_shared_tasks_activity_at ON shared_tasks(activity_at);
