-- Remove remote project cache
--
-- We now fetch swarm projects directly from the Hive instead of caching
-- remote project entries locally. This eliminates:
-- - UNIQUE constraint violations when local and remote have same remote_project_id
-- - Stale/wrong source_node_id for multi-node swarm projects
-- - Duplicated data that conflicts with hive

-- Delete all remote project cache entries
-- These were cached copies of projects from other nodes, now fetched directly from Hive
DELETE FROM projects WHERE is_remote = 1;

-- Drop the old unique index on remote_project_id
DROP INDEX IF EXISTS idx_projects_remote_project_id;

-- Create a simpler unique constraint that only applies to local projects
-- Only local projects (is_remote = 0) can have remote_project_id set
-- This ensures each local project can only be linked to one swarm project
CREATE UNIQUE INDEX idx_projects_remote_project_id
    ON projects(remote_project_id)
    WHERE remote_project_id IS NOT NULL AND is_remote = 0;
