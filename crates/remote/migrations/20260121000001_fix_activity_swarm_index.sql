-- Migration: Fix activity index ordering to match query pattern
-- Date: 2026-01-21
-- Issue: Index uses DESC ordering but queries use ASC, causing inefficient scans

-- Drop existing index
DROP INDEX IF EXISTS idx_activity_swarm_project_seq;

-- Recreate with correct ordering (ASC matches ORDER BY in queries)
-- This enables efficient index range scans for pagination:
--   WHERE swarm_project_id = $1 AND seq > $2 ORDER BY seq ASC
CREATE INDEX idx_activity_swarm_project_seq
ON activity(swarm_project_id, seq ASC)
WHERE swarm_project_id IS NOT NULL;
