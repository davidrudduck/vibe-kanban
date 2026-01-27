-- Add origin_node_id to task_attempts to track which node created the attempt
-- This enables hybrid local+Hive queries: local attempts are always queried from local DB,
-- while remote attempts (from other nodes) are fetched from Hive.
-- NULL origin_node_id is treated as "local" for backward compatibility with existing data.

ALTER TABLE task_attempts ADD COLUMN origin_node_id TEXT;

-- Index for filtering attempts by origin node
CREATE INDEX idx_task_attempts_origin_node_id ON task_attempts(origin_node_id);
