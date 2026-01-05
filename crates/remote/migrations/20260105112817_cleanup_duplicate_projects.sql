-- Migration: Cleanup duplicate and orphaned projects in the hive database
--
-- This fixes data integrity issues where:
-- 1. Multiple project records exist with the same name (duplicates)
-- 2. Projects exist without any node_projects link (orphans)
--
-- We keep one project per name, preferring:
-- - Projects that have a node_projects link
-- - Among those, the most recently created one

-- Step 1: Identify and delete duplicate projects, keeping the one with a node_projects link
-- or the most recently created one if none are linked
WITH ranked_projects AS (
    SELECT
        p.id,
        p.name,
        ROW_NUMBER() OVER (
            PARTITION BY p.name
            ORDER BY
                -- Prefer projects with a node_projects link
                CASE WHEN np.id IS NOT NULL THEN 0 ELSE 1 END,
                -- Then prefer the most recently created
                p.created_at DESC
        ) as rn
    FROM projects p
    LEFT JOIN node_projects np ON p.id = np.project_id
)
DELETE FROM projects
WHERE id IN (
    SELECT id FROM ranked_projects WHERE rn > 1
);

-- Step 2: Delete orphaned projects (those without any node_projects link)
-- These are projects that were created but never properly linked to a node
DELETE FROM projects p
WHERE NOT EXISTS (
    SELECT 1 FROM node_projects np WHERE np.project_id = p.id
);
