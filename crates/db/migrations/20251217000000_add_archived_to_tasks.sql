-- Add archived_at column to tasks table
-- This enables task archiving without deletion, preserving historical data
-- while allowing users to hide completed tasks from the active board

ALTER TABLE tasks ADD COLUMN archived_at TEXT;

-- Index for filtering archived/non-archived tasks efficiently
CREATE INDEX idx_tasks_archived_at ON tasks(archived_at);
