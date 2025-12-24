-- Add composite index for project filtering + activity_at sorting
--
-- This optimizes queries that filter by project_id and order by activity_at DESC,
-- such as Task::find_all_with_status() which uses:
--   WHERE project_id = ? ORDER BY COALESCE(activity_at, created_at) DESC
--
-- The simple idx_tasks_activity_at index only helps with global sorting.
-- This composite index covers the common case of project-scoped task listings.

CREATE INDEX IF NOT EXISTS idx_tasks_project_activity_at
ON tasks(project_id, activity_at DESC);
