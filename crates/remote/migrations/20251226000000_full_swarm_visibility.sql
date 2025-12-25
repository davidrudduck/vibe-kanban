-- Migration: Full Swarm Visibility
--
-- This migration enables full data sharing across the node swarm.
-- Previously, each project could only be linked to ONE node (UNIQUE constraint).
-- Now all nodes in an organization can see all projects, with ownership tracked
-- by the node_projects table.
--
-- The UNIQUE constraint on project_id is removed to allow future scenarios where
-- multiple nodes might have copies of the same project (e.g., for redundancy).
-- Currently, projects are still owned by a single node, but visibility is org-wide.

-- Drop the unique constraint on project_id
-- This allows the same project to potentially be linked to multiple nodes
-- (currently not used, but prepares for future multi-node project access)
ALTER TABLE node_projects DROP CONSTRAINT IF EXISTS node_projects_project_id_key;

-- Add an index for efficient lookups by project_id (replaces the unique index)
CREATE INDEX IF NOT EXISTS node_projects_project_id_idx ON node_projects(project_id);

-- Add comment explaining the visibility model
COMMENT ON TABLE node_projects IS 'Links projects to nodes that own them (have the git repo). All nodes in an organization can see all projects, but only the owning node can execute tasks.';
