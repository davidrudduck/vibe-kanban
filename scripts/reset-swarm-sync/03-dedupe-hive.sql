-- HIVE DEDUPLICATION SCRIPT
-- Run on TARDIS: docker exec -i remote-remote-db-1 psql -U remote -d remote < 03-dedupe-hive.sql
--
-- This removes duplicate shared_tasks, keeping the "original" (has attempts or earliest created_at)

BEGIN;

-- Create temp table of duplicates to delete
CREATE TEMP TABLE duplicates_to_delete AS
WITH task_groups AS (
    SELECT
        st.project_id,
        st.title,
        array_agg(st.id ORDER BY
            (SELECT COUNT(*) FROM node_task_attempts nta WHERE nta.shared_task_id = st.id) DESC,
            st.created_at ASC
        ) as ids_by_priority
    FROM shared_tasks st
    GROUP BY st.project_id, st.title
    HAVING COUNT(*) > 1
)
SELECT unnest(ids_by_priority[2:]) as id FROM task_groups;

-- Show what we'll delete
SELECT
    'WILL DELETE' as status,
    d.id,
    st.title,
    n.name as source_node,
    (SELECT COUNT(*) FROM node_task_attempts nta WHERE nta.shared_task_id = st.id) as attempts
FROM duplicates_to_delete d
JOIN shared_tasks st ON st.id = d.id
LEFT JOIN nodes n ON st.source_node_id = n.id
ORDER BY st.title;

-- Delete from node_task_output_logs (via assignments.task_id)
DELETE FROM node_task_output_logs
WHERE assignment_id IN (
    SELECT id FROM node_task_assignments WHERE task_id IN (SELECT id FROM duplicates_to_delete)
);

-- Delete from node_task_progress_events (via assignments.task_id)
DELETE FROM node_task_progress_events
WHERE assignment_id IN (
    SELECT id FROM node_task_assignments WHERE task_id IN (SELECT id FROM duplicates_to_delete)
);

-- Delete from node_execution_processes (via attempts)
DELETE FROM node_execution_processes
WHERE attempt_id IN (
    SELECT id FROM node_task_attempts WHERE shared_task_id IN (SELECT id FROM duplicates_to_delete)
);

-- Delete from node_task_attempts
DELETE FROM node_task_attempts WHERE shared_task_id IN (SELECT id FROM duplicates_to_delete);

-- Delete from shared_task_labels
DELETE FROM shared_task_labels WHERE shared_task_id IN (SELECT id FROM duplicates_to_delete);

-- Delete from node_task_assignments (uses task_id to reference shared_tasks)
DELETE FROM node_task_assignments WHERE task_id IN (SELECT id FROM duplicates_to_delete);

-- Delete the duplicate shared_tasks
DELETE FROM shared_tasks WHERE id IN (SELECT id FROM duplicates_to_delete);

DROP TABLE duplicates_to_delete;

COMMIT;

-- Report results
SELECT 'After deduplication:' as status;
SELECT COUNT(*) as shared_tasks FROM shared_tasks;
SELECT COUNT(*) as node_task_attempts FROM node_task_attempts;
