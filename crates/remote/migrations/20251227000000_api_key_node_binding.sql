-- Migration: API Key Node Binding
-- Implements "One API Key = One Node" identity model
--
-- This migration changes node identity from machine_id-based to API key-based:
-- - API keys become bound to specific nodes (first connection binds)
-- - machine_id becomes metadata only (no longer identity)
-- - Takeover detection fields prevent duplicate key use across machines

-- Add node_id column to node_api_keys (nullable for migration)
-- When a key is first used, it gets bound to the connecting node
ALTER TABLE node_api_keys
ADD COLUMN IF NOT EXISTS node_id UUID REFERENCES nodes(id) ON DELETE SET NULL;

-- Add takeover detection fields
-- These track when multiple machines attempt to use the same key
ALTER TABLE node_api_keys
ADD COLUMN IF NOT EXISTS takeover_count INTEGER NOT NULL DEFAULT 0;

ALTER TABLE node_api_keys
ADD COLUMN IF NOT EXISTS takeover_window_start TIMESTAMPTZ;

ALTER TABLE node_api_keys
ADD COLUMN IF NOT EXISTS blocked_at TIMESTAMPTZ;

ALTER TABLE node_api_keys
ADD COLUMN IF NOT EXISTS blocked_reason TEXT;

-- Index for efficiently looking up keys by node
CREATE INDEX IF NOT EXISTS idx_node_api_keys_node ON node_api_keys(node_id);

-- Remove machine_id from unique constraint (no longer identity)
-- machine_id is now just metadata for display purposes
ALTER TABLE nodes DROP CONSTRAINT IF EXISTS nodes_organization_id_machine_id_key;

-- Migration: Bind existing keys to nodes (best effort for single-node orgs)
-- For organizations with exactly one node and one active key, bind them together
UPDATE node_api_keys nak
SET node_id = (
    SELECT n.id FROM nodes n
    WHERE n.organization_id = nak.organization_id
    AND (SELECT COUNT(*) FROM nodes WHERE organization_id = nak.organization_id) = 1
    LIMIT 1
)
WHERE node_id IS NULL AND revoked_at IS NULL;

-- Note: Organizations with multiple nodes will need manual key assignment
-- or keys will be bound on first connection after migration
