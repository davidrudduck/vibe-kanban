-- Reset all remote_project_id values to start fresh with hive linking.
--
-- The hive linking system has had issues and this migration clears all
-- existing links so projects can be cleanly re-linked through the UI.
-- Also clears shared_task_id from tasks to avoid orphaned references.

-- Clear remote_project_id from all projects
UPDATE projects
SET remote_project_id = NULL
WHERE remote_project_id IS NOT NULL;

-- Clear shared_task_id from all tasks (these referenced the old hive tasks)
UPDATE tasks
SET shared_task_id = NULL
WHERE shared_task_id IS NOT NULL;
