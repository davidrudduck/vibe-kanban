-- Add archived_at column to shared_tasks table for task archiving sync across nodes.
-- NULL means the task is not archived; a timestamp means it was archived at that time.

ALTER TABLE shared_tasks ADD COLUMN archived_at TIMESTAMPTZ NULL;
