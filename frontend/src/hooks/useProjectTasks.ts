import { useCallback, useMemo, useState } from 'react';
import { useJsonPatchWsStream } from './useJsonPatchWsStream';
import {
  useRegisterOptimisticCallback,
  useRegisterStatusCallback,
  useRegisterArchivedCallback,
} from '@/contexts/TaskOptimisticContext';
import type { TaskStatus, TaskWithAttemptStatus } from 'shared/types';
import { sortTaskGroups, type SortDirection } from '@/lib/taskSorting';

type TasksState = {
  tasks: Record<string, TaskWithAttemptStatus>;
};

export interface UseProjectTasksResult {
  tasks: TaskWithAttemptStatus[];
  tasksById: Record<string, TaskWithAttemptStatus>;
  tasksByStatus: Record<TaskStatus, TaskWithAttemptStatus[]>;
  isLoading: boolean;
  isConnected: boolean;
  error: string | null;
  /**
   * Optimistically add a task to the local state.
   * Call this after successful REST API creation for instant UI feedback.
   */
  addTaskOptimistically: (task: TaskWithAttemptStatus) => void;
  /**
   * Optimistically update a task's status in the local state.
   * Call this after successful REST API status update for instant UI feedback.
   */
  updateTaskStatusOptimistically: (taskId: string, status: TaskStatus) => void;
  /**
   * Optimistically update a task's archived_at in the local state.
   * Call this after successful REST API archive/unarchive for instant UI feedback.
   */
  updateTaskArchivedOptimistically: (
    taskId: string,
    archivedAt: string | null
  ) => void;
}

export interface UseProjectTasksOptions {
  includeArchived?: boolean;
  /** Sort directions for each status column. Defaults to ascending (oldest first) for all. */
  sortDirections?: Partial<Record<TaskStatus, SortDirection>>;
}

/**
 * Stream tasks for a project via WebSocket (JSON Patch) and expose as array + map.
 * Server sends initial snapshot: replace /tasks with an object keyed by id.
 * Live updates arrive at /tasks/<id> via add/replace/remove operations.
 *
 * Note: remote_project_id is NOT passed to the backend - the backend fetches it
 * from the database using project_id. This avoids a race condition where
 * ProjectContext loads late, causing endpoint changes and WebSocket reconnection.
 */
export const useProjectTasks = (
  projectId: string,
  options: UseProjectTasksOptions = {}
): UseProjectTasksResult => {
  const { includeArchived = false, sortDirections } = options;
  const endpoint = projectId
    ? `/api/tasks/stream/ws?project_id=${encodeURIComponent(projectId)}&include_archived=${includeArchived}`
    : undefined;

  const initialData = useCallback((): TasksState => ({ tasks: {} }), []);

  const { data, isConnected, error, patchData } = useJsonPatchWsStream(
    endpoint,
    !!endpoint,
    initialData
  );

  // Track optimistic archived_at overrides that persist across WebSocket updates
  // Key: taskId, Value: { archivedAt: string | null, timestamp: number }
  // The timestamp helps us determine when to clear the override (after server confirms)
  const [optimisticArchivedOverrides, setOptimisticArchivedOverrides] =
    useState<Map<string, { archivedAt: string | null; timestamp: number }>>(
      () => new Map()
    );

  // Optimistically add a task to local state via JSON Patch
  const addTaskOptimistically = useCallback(
    (task: TaskWithAttemptStatus) => {
      patchData([
        {
          op: 'add',
          path: `/tasks/${task.id}`,
          value: task,
        },
      ]);
    },
    [patchData]
  );

  // Optimistically update a task's status via JSON Patch
  const updateTaskStatusOptimistically = useCallback(
    (taskId: string, status: TaskStatus) => {
      patchData([
        {
          op: 'replace',
          path: `/tasks/${taskId}/status`,
          value: status,
        },
      ]);
    },
    [patchData]
  );

  // Optimistically update a task's archived_at
  // This uses a separate state to persist across WebSocket updates
  const updateTaskArchivedOptimistically = useCallback(
    (taskId: string, archivedAt: string | null) => {
      // Store the optimistic override
      setOptimisticArchivedOverrides((prev) => {
        const next = new Map(prev);
        next.set(taskId, { archivedAt, timestamp: Date.now() });
        return next;
      });
    },
    []
  );

  // Register callbacks globally so modals/other components can access them
  useRegisterOptimisticCallback(projectId, addTaskOptimistically);
  useRegisterStatusCallback(projectId, updateTaskStatusOptimistically);
  useRegisterArchivedCallback(projectId, updateTaskArchivedOptimistically);

  // Merge WebSocket data with optimistic overrides
  const localTasksById = useMemo(() => {
    const tasks = data?.tasks ?? {};

    // If no overrides, return tasks as-is
    if (optimisticArchivedOverrides.size === 0) {
      return tasks;
    }

    // Apply optimistic overrides
    const merged: Record<string, TaskWithAttemptStatus> = {};
    for (const [taskId, task] of Object.entries(tasks)) {
      const override = optimisticArchivedOverrides.get(taskId);
      if (override) {
        // Check if server has confirmed our change
        // If server's archived_at matches our override (both null or both truthy), clear override
        const serverArchivedAt = task.archived_at;
        const optimisticArchivedAt = override.archivedAt;
        const serverConfirmed =
          (serverArchivedAt === null && optimisticArchivedAt === null) ||
          (serverArchivedAt !== null && optimisticArchivedAt !== null);

        if (serverConfirmed) {
          // Server confirmed, use server value and schedule override cleanup
          merged[taskId] = task;
          // Clean up this override asynchronously
          setTimeout(() => {
            setOptimisticArchivedOverrides((prev) => {
              const next = new Map(prev);
              // Only delete if it's the same override (hasn't been updated since)
              if (next.get(taskId)?.timestamp === override.timestamp) {
                next.delete(taskId);
              }
              return next;
            });
          }, 0);
        } else {
          // Apply optimistic override
          // Convert string to Date if truthy, keep null as null
          const archivedAtValue = optimisticArchivedAt
            ? new Date(optimisticArchivedAt)
            : null;
          merged[taskId] = { ...task, archived_at: archivedAtValue };
        }
      } else {
        merged[taskId] = task;
      }
    }

    return merged;
  }, [data?.tasks, optimisticArchivedOverrides]);

  const { tasks, tasksById, tasksByStatus } = useMemo(() => {
    const merged: Record<string, TaskWithAttemptStatus> = { ...localTasksById };
    const byStatus: Record<TaskStatus, TaskWithAttemptStatus[]> = {
      todo: [],
      inprogress: [],
      inreview: [],
      done: [],
      cancelled: [],
    };

    Object.values(merged).forEach((task) => {
      byStatus[task.status]?.push(task);
    });

    // Apply status-aware sorting using centralized utility
    const sortedByStatus = sortTaskGroups(byStatus, sortDirections);

    // Flatten sorted tasks for the 'tasks' array (most recent first overall)
    const sorted = Object.values(merged).sort((a, b) => {
      const aTime = new Date(a.created_at).getTime();
      const bTime = new Date(b.created_at).getTime();
      return bTime - aTime;
    });

    return { tasks: sorted, tasksById: merged, tasksByStatus: sortedByStatus };
  }, [localTasksById, sortDirections]);

  const isLoading = !data && !error; // until first snapshot

  return {
    tasks,
    tasksById,
    tasksByStatus,
    isLoading,
    isConnected,
    error,
    addTaskOptimistically,
    updateTaskStatusOptimistically,
    updateTaskArchivedOptimistically,
  };
};
