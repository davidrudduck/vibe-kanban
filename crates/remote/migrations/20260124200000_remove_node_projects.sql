-- Remove the legacy node_projects table.
--
-- The node_projects table was used to link nodes to projects before the swarm
-- architecture was implemented. It has been replaced by:
-- - swarm_project_nodes: Links nodes to swarm projects
-- - node_local_projects: Caches local project info for each node
--
-- All code has been migrated to use these tables instead.

-- First, drop the old FK constraint on node_task_assignments
ALTER TABLE node_task_assignments
    DROP CONSTRAINT IF EXISTS node_task_assignments_node_project_id_fkey;

-- Delete orphaned assignments that reference node_projects IDs (not in swarm_project_nodes).
-- These are stale assignments from before the swarm architecture migration.
DELETE FROM node_task_assignments
WHERE node_project_id NOT IN (SELECT id FROM swarm_project_nodes);

-- Now add the new FK constraint to swarm_project_nodes.
-- The column is now used to store swarm_project_nodes.id values (set in handle_attempt_sync).
ALTER TABLE node_task_assignments
    ADD CONSTRAINT node_task_assignments_node_project_id_fkey
    FOREIGN KEY (node_project_id) REFERENCES swarm_project_nodes(id) ON DELETE CASCADE;

-- Drop the legacy node_projects table
DROP TABLE IF EXISTS node_projects CASCADE;
