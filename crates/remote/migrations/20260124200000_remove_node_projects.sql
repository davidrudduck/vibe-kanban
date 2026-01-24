-- Remove the legacy node_projects table.
--
-- The node_projects table was used to link nodes to projects before the swarm
-- architecture was implemented. It has been replaced by:
-- - swarm_project_nodes: Links nodes to swarm projects
-- - node_local_projects: Caches local project info for each node
--
-- All code has been migrated to use these tables instead.

-- Drop the legacy node_projects table
DROP TABLE IF EXISTS node_projects CASCADE;
