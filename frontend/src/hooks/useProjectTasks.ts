import { useCallback, useMemo, useState, useEffect } from 'react';
import { useQuery } from '@tanstack/react-query';
import { useJsonPatchWsStream } from './useJsonPatchWsStream';
import {
  useRegisterOptimisticCallback,
  useRegisterStatusCallback,
  useRegisterArchivedCallback,
} from '@/contexts/TaskOptimisticContext';
import { tasksApi } from '@/lib/api';
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
  /** If true, use REST API polling instead of WebSocket (for remote swarm projects). */
  isRemote?: boolean;
  /** Polling interval for remote projects in ms (default: 5000). */
  pollingInterval?: number;
}

/**
 * Stream tasks for a project via WebSocket (JSON Patch) and expose as array + map.
 * Server sends initial snapshot: replace /tasks with an object keyed by id.
 * Live updates arrive at /tasks/<id> via add/replace/remove operations.
 *
 * For remote swarm projects (isRemote=true), falls back to REST API polling
 * since WebSocket only returns local tasks.
 *
 * Note: remote_project_id is NOT passed to the backend - the backend fetches it
 * from the database using project_id. This avoids a race condition where
 * ProjectContext loads late, causing endpoint changes and WebSocket reconnection.
 */
export const useProjectTasks = (
  projectId: string,
  options: UseProjectTasksOptions = {}
): UseProjectTasksResult => {
  const {
    includeArchived = false,
    sortDirections,
    isRemote = false,
    pollingInterval = 5000,
  } = options;

  // ============================================================================
  // WebSocket mode (local projects)
  // ============================================================================
  const wsEndpoint =
    projectId && !isRemote
      ? `/api/tasks/stream/ws?project_id=${encodeURIComponent(projectId)}&include_archived=${includeArchived}`
      : undefined;

  const initialData = useCallback((): TasksState => ({ tasks: {} }), []);

  const {
    data: wsData,
    isConnected: wsConnected,
    error: wsError,
    patchData,
  } = useJsonPatchWsStream(wsEndpoint, !!wsEndpoint, initialData);

  // ============================================================================
  // REST API mode (remote projects)
  // ============================================================================
  const {
    data: restData,
    isLoading: restLoading,
    error: restError,
    refetch: restRefetch,
  } = useQuery({
    queryKey: ['projectTasks', projectId, includeArchived],
    queryFn: () => tasksApi.listByProject(projectId, includeArchived),
    enabled: !!projectId && isRemote,
    staleTime: pollingInterval / 2, // Consider data stale after half the polling interval
    refetchInterval: pollingInterval,
    refetchIntervalInBackground: false, // Don't poll when tab is not active
  });

  // Convert REST array to record format for consistency
  const restTasksById = useMemo(() => {
    if (!restData) return {};
    const record: Record<string, TaskWithAttemptStatus> = {};
    for (const task of restData) {
      record[task.id] = task;
    }
    return record;
  }, [restData]);

  // ============================================================================
  // Optimistic updates state
  // ============================================================================

  // Track optimistic archived_at overrides that persist across updates
  // Key: taskId, Value: { archivedAt: string | null, timestamp: number }
  const [optimisticArchivedOverrides, setOptimisticArchivedOverrides] =
    useState<Map<string, { archivedAt: string | null; timestamp: number }>>(
      () => new Map()
    );

  // For remote mode, we need local state to support optimistic updates
  const [localOptimisticTasks, setLocalOptimisticTasks] = useState<
    Record<string, TaskWithAttemptStatus>
  >({});

  // Optimistically add a task to local state
  const addTaskOptimistically = useCallback(
    (task: TaskWithAttemptStatus) => {
      if (isRemote) {
        // For remote mode, add to local state
        setLocalOptimisticTasks((prev) => ({
          ...prev,
          [task.id]: task,
        }));
        // Trigger a refetch to sync with server
        restRefetch();
      } else {
        // For local mode, use JSON Patch
        patchData([
          {
            op: 'add',
            path: `/tasks/${task.id}`,
            value: task,
          },
        ]);
      }
    },
    [isRemote, patchData, restRefetch]
  );

  // Optimistically update a task's status
  const updateTaskStatusOptimistically = useCallback(
    (taskId: string, status: TaskStatus) => {
      if (isRemote) {
        // For remote mode, update local state
        setLocalOptimisticTasks((prev) => {
          const existing = prev[taskId] || restTasksById[taskId];
          if (!existing) return prev;
          return {
            ...prev,
            [taskId]: { ...existing, status },
          };
        });
        // Trigger a refetch to sync with server
        restRefetch();
      } else {
        // For local mode, use JSON Patch
        patchData([
          {
            op: 'replace',
            path: `/tasks/${taskId}/status`,
            value: status,
          },
        ]);
      }
    },
    [isRemote, patchData, restRefetch, restTasksById]
  );

  // Optimistically update a task's archived_at
  const updateTaskArchivedOptimistically = useCallback(
    (taskId: string, archivedAt: string | null) => {
      // Store the optimistic override (works for both modes)
      setOptimisticArchivedOverrides((prev) => {
        const next = new Map(prev);
        next.set(taskId, { archivedAt, timestamp: Date.now() });
        return next;
      });
      // For remote mode, also trigger a refetch
      if (isRemote) {
        restRefetch();
      }
    },
    [isRemote, restRefetch]
  );

  // Register callbacks globally so modals/other components can access them
  useRegisterOptimisticCallback(projectId, addTaskOptimistically);
  useRegisterStatusCallback(projectId, updateTaskStatusOptimistically);
  useRegisterArchivedCallback(projectId, updateTaskArchivedOptimistically);

  // Clear optimistic tasks when REST data changes (they've been synced)
  useEffect(() => {
    if (isRemote && restData) {
      // Only clear tasks that now exist in server data
      setLocalOptimisticTasks((prev) => {
        const newOptimistic: Record<string, TaskWithAttemptStatus> = {};
        for (const [id, task] of Object.entries(prev)) {
          if (!restData.some((t) => t.id === id)) {
            // Keep optimistic task if it's not in server data yet
            newOptimistic[id] = task;
          }
        }
        return newOptimistic;
      });
    }
  }, [isRemote, restData]);

  // Clear optimistic state when switching projects or toggling remote mode
  useEffect(() => {
    setLocalOptimisticTasks({});
    setOptimisticArchivedOverrides(new Map());
  }, [projectId, isRemote]);

  // ============================================================================
  // Merge data sources
  // ============================================================================

  // Select the appropriate data source
  const baseTasksById = useMemo(() => {
    if (isRemote) {
      // Merge REST data with optimistic local tasks
      return { ...restTasksById, ...localOptimisticTasks };
    } else {
      return wsData?.tasks ?? {};
    }
  }, [isRemote, restTasksById, localOptimisticTasks, wsData?.tasks]);

  // Merge base data with optimistic archived_at overrides
  const mergedTasksById = useMemo(() => {
    // If no overrides, return tasks as-is
    if (optimisticArchivedOverrides.size === 0) {
      return baseTasksById;
    }

    // Apply optimistic overrides
    const merged: Record<string, TaskWithAttemptStatus> = {};
    for (const [taskId, task] of Object.entries(baseTasksById)) {
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
  }, [baseTasksById, optimisticArchivedOverrides]);

  // ============================================================================
  // Final computed values
  // ============================================================================

  const { tasks, tasksById, tasksByStatus } = useMemo(() => {
    const merged: Record<string, TaskWithAttemptStatus> = {
      ...mergedTasksById,
    };
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
  }, [mergedTasksById, sortDirections]);

  // Determine loading and error states based on mode
  const isLoading = isRemote
    ? restLoading
    : !wsData && !wsError; // until first snapshot

  const isConnected = isRemote
    ? !restError // REST doesn't have "connected" concept, but no error = good
    : wsConnected;

  const error = isRemote
    ? restError
      ? String(restError)
      : null
    : wsError;

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
