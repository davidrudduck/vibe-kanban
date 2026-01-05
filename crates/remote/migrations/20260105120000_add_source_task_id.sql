-- Add source_task_id for task re-sync duplicate prevention
-- This column tracks the original local task ID that was synced to Hive,
-- allowing detection of duplicate re-sync attempts.

ALTER TABLE shared_tasks ADD COLUMN IF NOT EXISTS source_task_id UUID;
ALTER TABLE shared_tasks ADD COLUMN IF NOT EXISTS source_node_id UUID REFERENCES nodes(id) ON DELETE SET NULL;

-- Index for efficient lookup by source_task_id within a project
-- Used during re-sync to check if task was already re-created
CREATE INDEX IF NOT EXISTS idx_shared_tasks_source_task_id
    ON shared_tasks (project_id, source_node_id, source_task_id)
    WHERE source_task_id IS NOT NULL;

COMMENT ON COLUMN shared_tasks.source_task_id IS 'Original local task ID from source node, used for re-sync duplicate detection';
COMMENT ON COLUMN shared_tasks.source_node_id IS 'Node that originally created this task, used with source_task_id for uniqueness';
