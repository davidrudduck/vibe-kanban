-- Migration: Unlink Everything from Swarm
-- Part of Phase 1: Reset & Unlink Migration
-- Session 1.1: Create a clean slate by unlinking all existing data
--
-- This migration implements the "nothing linked by default" principle from the
-- Explicit Linking Architecture. After this migration, users must explicitly
-- configure what to share via the Swarm Management UI.

-- ============================================
-- Step 1: Unlink all projects from swarm
-- ============================================
-- Reset all remote/swarm-related fields on projects
-- Local projects (is_remote=0) will have their remote_project_id cleared
-- Remote projects (is_remote=1) will be deleted as they were cached from hive

-- First, delete remote projects (they're just cached copies from other nodes)
DELETE FROM projects WHERE is_remote = 1;

-- Unlink all remaining local projects from swarm
UPDATE projects SET
    remote_project_id = NULL,
    source_node_id = NULL,
    source_node_name = NULL,
    source_node_public_url = NULL,
    source_node_status = NULL,
    remote_last_synced_at = NULL
WHERE is_remote = 0;

-- ============================================
-- Step 2: Unlink all tasks from swarm
-- ============================================
-- First, delete remote tasks (they were cached from hive)
DELETE FROM tasks WHERE is_remote = 1;

-- Unlink all remaining local tasks
UPDATE tasks SET
    shared_task_id = NULL,
    remote_assignee_user_id = NULL,
    remote_assignee_name = NULL,
    remote_assignee_username = NULL,
    remote_version = 1,
    remote_last_synced_at = NULL,
    remote_stream_node_id = NULL,
    remote_stream_url = NULL
WHERE is_remote = 0;

-- ============================================
-- Step 3: Unlink all labels from swarm
-- ============================================
UPDATE labels SET
    shared_label_id = NULL,
    synced_at = NULL,
    version = 1;

-- ============================================
-- Step 4: Reset sync tracking on task attempts and execution data
-- ============================================
-- Clear hive sync tracking so data can be re-synced when explicitly linked
UPDATE task_attempts SET
    hive_synced_at = NULL,
    hive_assignment_id = NULL
WHERE hive_synced_at IS NOT NULL OR hive_assignment_id IS NOT NULL;

-- Reset log entry sync tracking
UPDATE log_entries SET
    hive_synced_at = NULL
WHERE hive_synced_at IS NOT NULL;

-- ============================================
-- Step 5: Drop shared_tasks cache table if it exists
-- ============================================
-- This table was used as a cache for remote tasks, no longer needed
-- with the unified tasks table approach
DROP TABLE IF EXISTS shared_tasks;

-- ============================================
-- Step 6: Clean up shared_activity_cursors if it exists
-- ============================================
DROP TABLE IF EXISTS shared_activity_cursors;

-- ============================================
-- Step 7: Clean up any cached node data
-- ============================================
DROP TABLE IF EXISTS cached_nodes;
