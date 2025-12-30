-- Add hive_assignment_id to task_attempts
-- This stores the assignment_id from the Hive for tasks dispatched by the Hive.
-- For locally-started tasks, this will be NULL until the Hive creates a synthetic assignment.

ALTER TABLE task_attempts ADD COLUMN hive_assignment_id TEXT;

-- Index for looking up attempts by their Hive assignment
CREATE INDEX IF NOT EXISTS idx_task_attempts_hive_assignment
    ON task_attempts(hive_assignment_id) WHERE hive_assignment_id IS NOT NULL;
