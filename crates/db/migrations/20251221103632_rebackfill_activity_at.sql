-- Re-backfill activity_at with status-aware timestamps
--
-- The initial backfill (20251218211726) used started_at for all tasks, but:
-- - Running tasks (inprogress) should use started_at (when execution began)
-- - Completed tasks (done, inreview) should use completed_at (when execution finished)
--
-- This caused tasks with old execution starts to appear above tasks with recent completions.

-- Update activity_at for inreview/done tasks to use completed_at instead of started_at
UPDATE tasks
SET activity_at = COALESCE(
  -- For done/inreview tasks: use completion time
  (SELECT ep.completed_at
   FROM task_attempts ta
   JOIN execution_processes ep ON ep.task_attempt_id = ta.id
   WHERE ta.task_id = tasks.id
     AND ep.completed_at IS NOT NULL
     AND ep.run_reason IN ('setupscript', 'cleanupscript', 'codingagent')
   ORDER BY ep.completed_at DESC
   LIMIT 1),
  -- Fallback to started_at if no completion time exists
  (SELECT ep.started_at
   FROM task_attempts ta
   JOIN execution_processes ep ON ep.task_attempt_id = ta.id
   WHERE ta.task_id = tasks.id
     AND ep.run_reason IN ('setupscript', 'cleanupscript', 'codingagent')
   ORDER BY ep.started_at DESC
   LIMIT 1),
  -- Final fallback to existing activity_at
  activity_at
)
WHERE status IN ('inreview', 'done');
