-- Migration: Fix is_remote flag for tasks belonging to local projects
--
-- Problem: Some tasks were incorrectly marked as is_remote = 1 even though
-- they belong to local projects (projects with is_remote = 0).
--
-- This can happen when:
-- 1. A local task is synced through the wrong code path
-- 2. A task was created during a sync process that incorrectly set the flag
-- 3. The upsert_remote_task ON CONFLICT clause overwrote a local task
--
-- Fix: Reset is_remote = 0 for all tasks that belong to local projects
-- (projects where is_remote = 0 AND remote_project_id IS NOT NULL)

UPDATE tasks
SET is_remote = 0, updated_at = datetime('now', 'subsec')
WHERE is_remote = 1
  AND project_id IN (
      SELECT id FROM projects
      WHERE is_remote = 0
        AND remote_project_id IS NOT NULL
  );
