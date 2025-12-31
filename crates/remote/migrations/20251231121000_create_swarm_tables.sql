-- Migration: Create Swarm Tables (PostgreSQL/Hive)
-- Part of Phase 1: Reset & Unlink Migration
-- Session 1.2: Create new swarm_* tables and rename existing tables
--
-- This migration creates the new swarm architecture:
-- - swarm_projects: Renamed from projects for clarity
-- - swarm_project_nodes: Junction table linking swarm projects to node projects
-- - swarm_tasks: Renamed from shared_tasks

-- ============================================
-- Step 1: Rename shared_tasks to swarm_tasks
-- ============================================
-- First, drop the foreign key constraints that reference shared_tasks
ALTER TABLE node_task_assignments DROP CONSTRAINT IF EXISTS node_task_assignments_task_id_fkey;

-- Rename the table
ALTER TABLE shared_tasks RENAME TO swarm_tasks;

-- Rename column project_id to swarm_project_id for consistency
-- (though it still references projects table for now)
-- We'll leave it as project_id since it references the hive's projects table

-- Recreate indexes with new names
DROP INDEX IF EXISTS idx_tasks_org_status;
DROP INDEX IF EXISTS idx_tasks_org_assignee;
DROP INDEX IF EXISTS idx_tasks_project;
DROP INDEX IF EXISTS idx_shared_tasks_org_deleted_at;

CREATE INDEX IF NOT EXISTS idx_swarm_tasks_org_status ON swarm_tasks(organization_id, status);
CREATE INDEX IF NOT EXISTS idx_swarm_tasks_org_assignee ON swarm_tasks(organization_id, assignee_user_id);
CREATE INDEX IF NOT EXISTS idx_swarm_tasks_project ON swarm_tasks(project_id);
CREATE INDEX IF NOT EXISTS idx_swarm_tasks_org_deleted_at ON swarm_tasks(organization_id, deleted_at)
    WHERE deleted_at IS NOT NULL;

-- ============================================
-- Step 2: Update node_task_assignments to reference swarm_tasks
-- ============================================
-- Recreate the foreign key constraint to point to swarm_tasks
ALTER TABLE node_task_assignments
    ADD CONSTRAINT node_task_assignments_task_id_fkey
    FOREIGN KEY (task_id) REFERENCES swarm_tasks(id) ON DELETE CASCADE;

-- ============================================
-- Step 3: Create swarm_project_nodes junction table
-- ============================================
-- This replaces the 1:1 node_projects with a many-to-many relationship
-- One swarm project can be linked to multiple nodes
CREATE TABLE IF NOT EXISTS swarm_project_nodes (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    -- References the hive's projects table (swarm project)
    swarm_project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    -- References the node
    node_id UUID NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
    -- ID in node's SQLite database
    local_project_id UUID NOT NULL,
    -- Path to git repo on the node
    git_repo_path TEXT NOT NULL,
    -- Operating system type for path handling
    os_type TEXT,  -- 'linux', 'darwin', 'windows'
    -- When this link was created
    linked_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- Ensure a node can only link a specific local project to a swarm project once
    UNIQUE(swarm_project_id, node_id, local_project_id)
);

CREATE INDEX IF NOT EXISTS idx_swarm_project_nodes_swarm_project ON swarm_project_nodes(swarm_project_id);
CREATE INDEX IF NOT EXISTS idx_swarm_project_nodes_node ON swarm_project_nodes(node_id);
CREATE INDEX IF NOT EXISTS idx_swarm_project_nodes_local_project ON swarm_project_nodes(local_project_id);

-- ============================================
-- Step 4: Update shared_task_labels to swarm_task_labels
-- ============================================
-- Rename the junction table if it exists
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'shared_task_labels') THEN
        ALTER TABLE shared_task_labels RENAME TO swarm_task_labels;
    END IF;
END
$$;

-- ============================================
-- Step 5: Update triggers
-- ============================================
-- Drop old trigger and create new one
DROP TRIGGER IF EXISTS trg_shared_tasks_updated_at ON swarm_tasks;

CREATE TRIGGER trg_swarm_tasks_updated_at
    BEFORE UPDATE ON swarm_tasks
    FOR EACH ROW
    EXECUTE FUNCTION set_updated_at();

-- ============================================
-- Step 6: Rename labels.origin_node_id to source_node_id for consistency
-- ============================================
-- Check if the column exists before renaming
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'labels' AND column_name = 'origin_node_id'
    ) THEN
        ALTER TABLE labels RENAME COLUMN origin_node_id TO source_node_id;
    END IF;
END
$$;

-- ============================================
-- Step 7: Add comments for documentation
-- ============================================
COMMENT ON TABLE swarm_tasks IS 'Tasks shared across the swarm, synced to all linked nodes';
COMMENT ON TABLE swarm_project_nodes IS 'Links swarm projects to node projects (many-to-many)';
COMMENT ON COLUMN swarm_project_nodes.swarm_project_id IS 'References the shared project in the hive';
COMMENT ON COLUMN swarm_project_nodes.local_project_id IS 'UUID of the project in the node SQLite database';
COMMENT ON COLUMN swarm_project_nodes.os_type IS 'Operating system type for path handling';
