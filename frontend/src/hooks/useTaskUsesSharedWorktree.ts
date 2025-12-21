import { useMemo } from 'react';
import { useTask, useTaskAttempts } from '@/hooks';

/**
 * Determines whether a task uses a shared worktree with its parent.
 *
 * A task uses a shared worktree if:
 * 1. It has a parent_task_id
 * 2. Any of its attempts shares the same container_ref with any of the parent's attempts
 *
 * This is used to prevent creating subtasks for tasks that use a shared worktree,
 * as this would create problematic chains.
 */
export function useTaskUsesSharedWorktree(
  taskId?: string,
  opts?: { enabled?: boolean }
) {
  const enabled = (opts?.enabled ?? true) && !!taskId;

  const { data: task, isLoading: isLoadingTask } = useTask(taskId, { enabled });

  const parentTaskId = task?.parent_task_id ?? undefined;

  const { data: taskAttempts = [], isLoading: isLoadingTaskAttempts } =
    useTaskAttempts(taskId, { enabled, refetchInterval: false });

  const { data: parentAttempts = [], isLoading: isLoadingParentAttempts } =
    useTaskAttempts(parentTaskId, {
      enabled: enabled && !!parentTaskId,
      refetchInterval: false,
    });

  const isLoading = isLoadingTask || isLoadingTaskAttempts || isLoadingParentAttempts;

  const usesSharedWorktree = useMemo(() => {
    // No parent means cannot use shared worktree
    if (!parentTaskId) return false;

    // No attempts means cannot determine shared worktree (default to false)
    if (taskAttempts.length === 0 || parentAttempts.length === 0) return false;

    // Get all valid container_refs from parent attempts
    const parentContainerRefs = new Set(
      parentAttempts
        .map((attempt) => attempt.container_ref)
        .filter((ref): ref is string => ref != null)
    );

    // Check if any task attempt shares a container_ref with parent
    return taskAttempts.some(
      (attempt) =>
        attempt.container_ref != null &&
        parentContainerRefs.has(attempt.container_ref)
    );
  }, [parentTaskId, taskAttempts, parentAttempts]);

  return {
    usesSharedWorktree,
    isLoading,
    // Also expose parent task ID for consumers that need it
    parentTaskId,
  };
}
