-- Add backfill_request_id column to track correlation between backfill requests and attempts
-- This allows the hive to look up attempt IDs from the database when in-memory tracker state is lost
-- (e.g., due to node disconnect before response arrives)

ALTER TABLE node_task_attempts ADD COLUMN backfill_request_id UUID;

-- Partial index for efficient lookup of attempts by backfill request ID
-- Only indexes rows where backfill_request_id is set (during pending_backfill state)
CREATE INDEX idx_node_task_attempts_backfill_request
  ON node_task_attempts (backfill_request_id)
  WHERE backfill_request_id IS NOT NULL;
