-- Add executing_node_id to track which node is currently executing a task
-- This enables the frontend to show which node a task was dispatched to

ALTER TABLE shared_tasks
    ADD COLUMN executing_node_id UUID REFERENCES nodes(id) ON DELETE SET NULL;

-- Index for efficient lookups of tasks by executing node
CREATE INDEX IF NOT EXISTS idx_shared_tasks_executing_node
    ON shared_tasks(executing_node_id)
    WHERE executing_node_id IS NOT NULL;

COMMENT ON COLUMN shared_tasks.executing_node_id IS
    'The node currently executing this task (set when an attempt is started)';
