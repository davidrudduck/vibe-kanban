-- Remove legacy projects system - swarm_projects is now the single source of truth.
--
-- This migration:
-- 1. Drops foreign key constraints that reference the projects table
-- 2. Makes project_id nullable in shared_tasks (swarm_project_id is now the source of truth)
-- 3. Drops the legacy project_activity_counters table
-- 4. Drops the legacy projects table
-- 5. Recreates project_activity_counters using project_id as key (for legacy compatibility)

-- First, drop foreign key constraints
ALTER TABLE shared_tasks DROP CONSTRAINT IF EXISTS shared_tasks_project_id_fkey;
ALTER TABLE activity DROP CONSTRAINT IF EXISTS activity_project_id_fkey;
ALTER TABLE node_projects DROP CONSTRAINT IF EXISTS node_projects_project_id_fkey;

-- Make project_id nullable in shared_tasks (swarm_project_id is now the source of truth)
ALTER TABLE shared_tasks ALTER COLUMN project_id DROP NOT NULL;

-- Drop the old project_activity_counters table
DROP TABLE IF EXISTS project_activity_counters;

-- Drop the legacy projects table
DROP TABLE IF EXISTS projects CASCADE;

-- Recreate project_activity_counters using project_id as key (for legacy activity tracking)
-- This allows the activity system to continue working with project_id as the key
CREATE TABLE IF NOT EXISTS project_activity_counters (
    project_id UUID PRIMARY KEY,
    swarm_project_id UUID REFERENCES swarm_projects(id) ON DELETE CASCADE,
    last_seq BIGINT NOT NULL DEFAULT 0
);

-- Create index for swarm_project_id lookups
CREATE INDEX IF NOT EXISTS idx_project_activity_counters_swarm
    ON project_activity_counters(swarm_project_id)
    WHERE swarm_project_id IS NOT NULL;

-- Drop the old projects index if it exists
DROP INDEX IF EXISTS idx_projects_org_name;
