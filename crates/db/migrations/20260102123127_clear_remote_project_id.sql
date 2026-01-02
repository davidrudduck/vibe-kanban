-- Clear all remote_project_id values to reset legacy shared project links
-- This migration makes all existing projects local-only as part of the transition
-- to the new Swarm management system. Projects can be re-linked to Swarm projects
-- through the new NodeProjectsSection interface.

UPDATE projects
SET remote_project_id = NULL
WHERE remote_project_id IS NOT NULL;
