-- HIVE DEDUPLICATION SCRIPT
-- Run on TARDIS: docker exec -i remote-remote-db-1 psql -U remote -d remote < 03-dedupe-hive.sql
--
-- This removes duplicate shared_tasks, keeping the "original" (has attempts or earliest created_at)
-- Duplicates are created when nodes receive tasks from hive and sync them back as new entries.

BEGIN;

-- Step 1: Identify duplicates (same project + title, different source_node)
-- The "original" is the one with attempts, or earliest created_at if no attempts
WITH task_groups AS (
    SELECT
        st.project_id,
        st.title,
        COUNT(*) as copies,
        array_agg(st.id ORDER BY
            (SELECT COUNT(*) FROM node_task_attempts nta WHERE nta.shared_task_id = st.id) DESC,
            st.created_at ASC
        ) as ids_by_priority
    FROM shared_tasks st
    GROUP BY st.project_id, st.title
    HAVING COUNT(*) > 1
),
originals AS (
    -- First element is the original (most attempts, then earliest)
    SELECT project_id, title, ids_by_priority[1] as original_id
    FROM task_groups
),
duplicates AS (
    -- All other elements are duplicates
    SELECT tg.project_id, tg.title, unnest(tg.ids_by_priority[2:]) as duplicate_id
    FROM task_groups tg
)
-- Show what we'll delete
SELECT
    'DUPLICATE' as status,
    d.duplicate_id,
    st.title,
    n.name as source_node,
    st.created_at,
    (SELECT COUNT(*) FROM node_task_attempts nta WHERE nta.shared_task_id = st.id) as attempts
FROM duplicates d
JOIN shared_tasks st ON st.id = d.duplicate_id
LEFT JOIN nodes n ON st.source_node_id = n.id
ORDER BY st.title, st.created_at;

-- Step 2: Delete duplicates (cascade handles dependent tables)
-- First delete dependent records
DELETE FROM node_task_output_logs
WHERE attempt_id IN (
    SELECT nta.id FROM node_task_attempts nta
    WHERE nta.shared_task_id IN (
        SELECT unnest(ids_by_priority[2:]) FROM (
            SELECT array_agg(st.id ORDER BY
                (SELECT COUNT(*) FROM node_task_attempts nta2 WHERE nta2.shared_task_id = st.id) DESC,
                st.created_at ASC
            ) as ids_by_priority
            FROM shared_tasks st
            GROUP BY st.project_id, st.title
            HAVING COUNT(*) > 1
        ) tg
    )
);

DELETE FROM node_execution_processes
WHERE attempt_id IN (
    SELECT nta.id FROM node_task_attempts nta
    WHERE nta.shared_task_id IN (
        SELECT unnest(ids_by_priority[2:]) FROM (
            SELECT array_agg(st.id ORDER BY
                (SELECT COUNT(*) FROM node_task_attempts nta2 WHERE nta2.shared_task_id = st.id) DESC,
                st.created_at ASC
            ) as ids_by_priority
            FROM shared_tasks st
            GROUP BY st.project_id, st.title
            HAVING COUNT(*) > 1
        ) tg
    )
);

DELETE FROM node_task_progress_events
WHERE attempt_id IN (
    SELECT nta.id FROM node_task_attempts nta
    WHERE nta.shared_task_id IN (
        SELECT unnest(ids_by_priority[2:]) FROM (
            SELECT array_agg(st.id ORDER BY
                (SELECT COUNT(*) FROM node_task_attempts nta2 WHERE nta2.shared_task_id = st.id) DESC,
                st.created_at ASC
            ) as ids_by_priority
            FROM shared_tasks st
            GROUP BY st.project_id, st.title
            HAVING COUNT(*) > 1
        ) tg
    )
);

DELETE FROM node_task_attempts
WHERE shared_task_id IN (
    SELECT unnest(ids_by_priority[2:]) FROM (
        SELECT array_agg(st.id ORDER BY
            (SELECT COUNT(*) FROM node_task_attempts nta2 WHERE nta2.shared_task_id = st.id) DESC,
            st.created_at ASC
        ) as ids_by_priority
        FROM shared_tasks st
        GROUP BY st.project_id, st.title
        HAVING COUNT(*) > 1
    ) tg
);

DELETE FROM shared_task_labels
WHERE shared_task_id IN (
    SELECT unnest(ids_by_priority[2:]) FROM (
        SELECT array_agg(st.id ORDER BY
            (SELECT COUNT(*) FROM node_task_attempts nta2 WHERE nta2.shared_task_id = st.id) DESC,
            st.created_at ASC
        ) as ids_by_priority
        FROM shared_tasks st
        GROUP BY st.project_id, st.title
        HAVING COUNT(*) > 1
    ) tg
);

DELETE FROM node_task_assignments
WHERE shared_task_id IN (
    SELECT unnest(ids_by_priority[2:]) FROM (
        SELECT array_agg(st.id ORDER BY
            (SELECT COUNT(*) FROM node_task_attempts nta2 WHERE nta2.shared_task_id = st.id) DESC,
            st.created_at ASC
        ) as ids_by_priority
        FROM shared_tasks st
        GROUP BY st.project_id, st.title
        HAVING COUNT(*) > 1
    ) tg
);

-- Finally delete the duplicate shared_tasks
DELETE FROM shared_tasks
WHERE id IN (
    SELECT unnest(ids_by_priority[2:]) FROM (
        SELECT array_agg(st.id ORDER BY
            (SELECT COUNT(*) FROM node_task_attempts nta2 WHERE nta2.shared_task_id = st.id) DESC,
            st.created_at ASC
        ) as ids_by_priority
        FROM shared_tasks st
        GROUP BY st.project_id, st.title
        HAVING COUNT(*) > 1
    ) tg
);

COMMIT;

-- Report results
SELECT 'After deduplication:' as status;
SELECT COUNT(*) as shared_tasks FROM shared_tasks;
SELECT COUNT(*) as node_task_attempts FROM node_task_attempts;
