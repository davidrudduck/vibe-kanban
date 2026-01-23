-- Fix swarm task synchronization by adding swarm_project_id to shared_tasks.
-- This allows tasks to be synced via swarm_projects instead of the legacy projects table.
--
-- The new flow is:
-- 1. Node sends TaskSync with local_project_id (its project.id)
-- 2. Hive looks up node_local_projects → swarm_project_nodes → swarm_projects
-- 3. Task is created with swarm_project_id (and project_id for backwards compat)
--
-- This migration also adds owner tracking fields for tasks.

-- Add swarm_project_id to shared_tasks (nullable to support migration, can be made NOT NULL later)
ALTER TABLE shared_tasks ADD COLUMN IF NOT EXISTS swarm_project_id UUID REFERENCES swarm_projects(id) ON DELETE SET NULL;

-- Add owner tracking fields (which node is currently working on this task)
ALTER TABLE shared_tasks ADD COLUMN IF NOT EXISTS owner_node_id UUID REFERENCES nodes(id) ON DELETE SET NULL;
ALTER TABLE shared_tasks ADD COLUMN IF NOT EXISTS owner_name TEXT;

-- Create index for querying tasks by swarm project
CREATE INDEX IF NOT EXISTS idx_shared_tasks_swarm_project
    ON shared_tasks(swarm_project_id)
    WHERE swarm_project_id IS NOT NULL AND deleted_at IS NULL;

-- Create unique constraint on (source_node_id, source_task_id) to prevent duplicate syncs
-- This replaces the (project_id, source_node_id, source_task_id) logic in find_by_source_task_id
CREATE UNIQUE INDEX IF NOT EXISTS idx_shared_tasks_source_unique
    ON shared_tasks(source_node_id, source_task_id)
    WHERE source_node_id IS NOT NULL AND source_task_id IS NOT NULL AND deleted_at IS NULL;

-- Update activity table to support swarm_project_id lookup
-- (Already has swarm_project_id from previous migration, just add index if missing)
CREATE INDEX IF NOT EXISTS idx_activity_swarm_project_created
    ON activity(swarm_project_id, created_at)
    WHERE swarm_project_id IS NOT NULL;

-- Add is_owner flag to swarm_project_nodes to track which node owns the git repo
ALTER TABLE swarm_project_nodes ADD COLUMN IF NOT EXISTS is_owner BOOLEAN NOT NULL DEFAULT false;
