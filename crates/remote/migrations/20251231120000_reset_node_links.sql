-- Migration: Reset Node Links (PostgreSQL/Hive)
-- Part of Phase 1: Reset & Unlink Migration
-- Session 1.1: Clear node-project links to support explicit linking architecture
--
-- This migration clears the node_projects table so that users can explicitly
-- configure which projects to share via the Swarm Management UI.
-- The nodes themselves remain registered, but their project links are cleared.

-- ============================================
-- Step 1: Clear node-project links
-- ============================================
-- Delete all node-project associations
-- Users will re-link projects explicitly via Swarm Management
DELETE FROM node_projects;

-- Note: We keep the following tables intact:
-- - nodes: Node registrations remain valid
-- - node_api_keys: API keys remain valid
-- - shared_tasks: Tasks are kept for historical data; can be re-linked
-- - projects: Hive projects are kept; nodes will link to them explicitly
-- - labels: Labels are kept; can be re-linked via Swarm Management

-- ============================================
-- Step 2: Clear task assignments
-- ============================================
-- Clear task assignments since they depend on node_projects
DELETE FROM node_task_assignments;

-- ============================================
-- Step 3: Reset node sync status
-- ============================================
-- Mark all nodes as offline since their projects are now unlinked
UPDATE nodes SET status = 'offline' WHERE status != 'offline';
