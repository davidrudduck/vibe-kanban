-- Add activity_at column to tasks table
-- This column tracks when the task last had a significant activity (status change)
-- Unlike updated_at, this is NOT updated for metadata changes like title/description edits

-- Add the column (nullable initially for backfill)
ALTER TABLE tasks ADD COLUMN activity_at TEXT;

-- Backfill from execution_processes: use the latest execution start time for each task
-- For tasks without execution processes, leave NULL (will fallback to updated_at in queries)
UPDATE tasks
SET activity_at = (
    SELECT ep.started_at
    FROM task_attempts ta
    JOIN execution_processes ep ON ep.task_attempt_id = ta.id
    WHERE ta.task_id = tasks.id
      AND ep.run_reason IN ('setupscript', 'cleanupscript', 'codingagent')
    ORDER BY ep.started_at DESC
    LIMIT 1
);

-- For tasks without execution processes, use created_at as the initial activity time
UPDATE tasks
SET activity_at = created_at
WHERE activity_at IS NULL;

-- Create index for efficient ordering by activity_at
CREATE INDEX idx_tasks_activity_at ON tasks(activity_at);
