-- HIVE RESET SCRIPT
-- Run on TARDIS: docker exec -i remote-remote-db-1 psql -U remote -d remote < 01-clear-hive.sql
--
-- This clears all shared task data from the hive to allow a fresh re-sync from nodes.
-- WARNING: This deletes ALL task sync data. Nodes will re-push their local tasks.

BEGIN;

-- Clear dependent tables first (FK constraints)
DELETE FROM node_task_output_logs;
DELETE FROM node_execution_processes;
DELETE FROM node_task_progress_events;
DELETE FROM node_task_attempts;
DELETE FROM shared_task_labels;
DELETE FROM node_task_assignments;

-- Clear main shared_tasks table
DELETE FROM shared_tasks;

-- Reset activity cursors so nodes re-fetch from beginning
DELETE FROM project_activity_counters;

COMMIT;

-- Verify cleanup
SELECT 'shared_tasks' as table_name, COUNT(*) as remaining FROM shared_tasks
UNION ALL
SELECT 'node_task_attempts', COUNT(*) FROM node_task_attempts
UNION ALL
SELECT 'shared_task_labels', COUNT(*) FROM shared_task_labels;
