import { useCallback, useMemo } from 'react';
import { useJsonPatchWsStream } from './useJsonPatchWsStream';
import {
  useRegisterOptimisticCallback,
  useRegisterStatusCallback,
} from '@/contexts/TaskOptimisticContext';
import type {
  SharedTask,
  TaskStatus,
  TaskWithAttemptStatus,
} from 'shared/types';

export type SharedTaskRecord = Omit<
  SharedTask,
  'version' | 'last_event_seq'
> & {
  version: number;
  last_event_seq: number | null;
  created_at: string | Date;
  updated_at: string | Date;
  assignee_first_name?: string | null;
  assignee_last_name?: string | null;
  assignee_username?: string | null;
};

type TasksState = {
  tasks: Record<string, TaskWithAttemptStatus>;
  shared_tasks: Record<string, SharedTaskRecord>;
};

export interface UseProjectTasksResult {
  tasks: TaskWithAttemptStatus[];
  tasksById: Record<string, TaskWithAttemptStatus>;
  tasksByStatus: Record<TaskStatus, TaskWithAttemptStatus[]>;
  sharedTasksById: Record<string, SharedTaskRecord>;
  sharedOnlyByStatus: Record<TaskStatus, SharedTaskRecord[]>;
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
}

export interface UseProjectTasksOptions {
  includeArchived?: boolean;
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
  const { includeArchived = false } = options;
  const endpoint = projectId
    ? `/api/tasks/stream/ws?project_id=${encodeURIComponent(projectId)}&include_archived=${includeArchived}`
    : undefined;

  const initialData = useCallback(
    (): TasksState => ({ tasks: {}, shared_tasks: {} }),
    []
  );

  const { data, isConnected, error, patchData } = useJsonPatchWsStream(
    endpoint,
    !!endpoint,
    initialData
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

  // Register callbacks globally so modals/other components can access them
  useRegisterOptimisticCallback(projectId, addTaskOptimistically);
  useRegisterStatusCallback(projectId, updateTaskStatusOptimistically);

  const localTasksById = useMemo(() => data?.tasks ?? {}, [data?.tasks]);
  const sharedTasksById = useMemo(
    () => data?.shared_tasks ?? {},
    [data?.shared_tasks]
  );

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

    // Helper: get activity time (fallback to created_at)
    const getActivityTime = (task: TaskWithAttemptStatus) =>
      new Date(
        ((task.activity_at ?? task.created_at) as string | Date).toString()
      ).getTime();

    const sorted = Object.values(merged).sort(
      (a, b) => getActivityTime(b) - getActivityTime(a)
    );

    // Apply status-aware sorting:
    // - Todo: oldest first (FIFO queue - prevents older tasks from being buried)
    // - All others: most recent activity first
    const TASK_STATUSES: TaskStatus[] = [
      'todo',
      'inprogress',
      'inreview',
      'done',
      'cancelled',
    ];
    TASK_STATUSES.forEach((status) => {
      if (status === 'todo') {
        // Todo: oldest first (ascending by activity_at)
        byStatus[status].sort(
          (a, b) => getActivityTime(a) - getActivityTime(b)
        );
      } else {
        // All others: most recent first (descending by activity_at)
        byStatus[status].sort(
          (a, b) => getActivityTime(b) - getActivityTime(a)
        );
      }
    });

    return { tasks: sorted, tasksById: merged, tasksByStatus: byStatus };
  }, [localTasksById]);

  const sharedOnlyByStatus = useMemo(() => {
    const grouped: Record<TaskStatus, SharedTaskRecord[]> = {
      todo: [],
      inprogress: [],
      inreview: [],
      done: [],
      cancelled: [],
    };

    // Build a set of shared_task_ids that are already represented in local tasks
    // This ensures we don't show duplicates when a task exists in both
    // the local tasks table (with is_remote=true) AND the shared_tasks table
    const referencedSharedIds = new Set(
      Object.values(localTasksById)
        .map((task) => task.shared_task_id)
        .filter((id): id is string => Boolean(id))
    );

    Object.values(sharedTasksById).forEach((sharedTask) => {
      // Skip this shared task if its ID matches a local task's shared_task_id
      // This properly deduplicates remote tasks that appear in both tables
      if (referencedSharedIds.has(sharedTask.id)) {
        return;
      }
      grouped[sharedTask.status]?.push(sharedTask);
    });

    // Helper: get activity time for shared tasks (fallback to created_at)
    const getSharedActivityTime = (task: SharedTaskRecord) =>
      new Date(
        ((task.activity_at ?? task.created_at) as string | Date).toString()
      ).getTime();

    // Apply same status-aware sorting as local tasks
    const TASK_STATUSES: TaskStatus[] = [
      'todo',
      'inprogress',
      'inreview',
      'done',
      'cancelled',
    ];
    TASK_STATUSES.forEach((status) => {
      if (status === 'todo') {
        // Todo: oldest first (ascending by activity_at)
        grouped[status].sort(
          (a, b) => getSharedActivityTime(a) - getSharedActivityTime(b)
        );
      } else {
        // All others: most recent first (descending by activity_at)
        grouped[status].sort(
          (a, b) => getSharedActivityTime(b) - getSharedActivityTime(a)
        );
      }
    });

    return grouped;
  }, [localTasksById, sharedTasksById]);

  const isLoading = !data && !error; // until first snapshot

  return {
    tasks,
    tasksById,
    tasksByStatus,
    sharedTasksById,
    sharedOnlyByStatus,
    isLoading,
    isConnected,
    error,
    addTaskOptimistically,
    updateTaskStatusOptimistically,
  };
};
