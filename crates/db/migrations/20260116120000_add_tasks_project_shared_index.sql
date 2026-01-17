-- Add composite index on tasks table for optimised swarm sync health queries
-- Filters by project_id and checks for non-null shared_task_id values
CREATE INDEX IF NOT EXISTS idx_tasks_project_shared
ON tasks(project_id, shared_task_id)
WHERE shared_task_id IS NOT NULL;
