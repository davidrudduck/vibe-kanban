-- Add sync state tracking to node_task_attempts
-- This tracks whether all data for an attempt has been fully synchronized from the node

-- sync_state values:
--   'partial'          - Initial/default state, data may be incomplete
--   'pending_backfill' - Backfill has been requested, awaiting response
--   'complete'         - All data confirmed synchronized

ALTER TABLE node_task_attempts
ADD COLUMN IF NOT EXISTS sync_state VARCHAR(20) NOT NULL DEFAULT 'partial',
ADD COLUMN IF NOT EXISTS sync_requested_at TIMESTAMPTZ,
ADD COLUMN IF NOT EXISTS last_full_sync_at TIMESTAMPTZ;

-- Index for finding incomplete attempts (for periodic reconciliation)
CREATE INDEX IF NOT EXISTS idx_node_task_attempts_incomplete
ON node_task_attempts (node_id, sync_state)
WHERE sync_state != 'complete';

-- Index for finding attempts pending backfill (to avoid duplicate requests)
CREATE INDEX IF NOT EXISTS idx_node_task_attempts_pending
ON node_task_attempts (node_id)
WHERE sync_state = 'pending_backfill';
