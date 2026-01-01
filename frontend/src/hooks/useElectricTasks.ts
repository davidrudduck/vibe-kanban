/**
 * useElectricTasks - Real-time task sync using Electric SQL
 *
 * This hook provides real-time task synchronization using Electric SQL
 * and TanStack DB collections. It replaces the WebSocket-based sync
 * with a more efficient HTTP shape streaming approach.
 *
 * Features:
 * - Real-time sync from PostgreSQL via Electric
 * - Automatic filtering by project
 * - Tasks grouped by status
 * - Excludes deleted tasks by default
 *
 * @example
 * ```tsx
 * const { tasks, tasksByStatus, isLoading } = useElectricTasks('project-1');
 *
 * // Render tasks in a Kanban board
 * <Column status="todo" tasks={tasksByStatus.todo} />
 * <Column status="inprogress" tasks={tasksByStatus.inprogress} />
 * ```
 */

import { useMemo } from 'react';
import { useShape } from '@electric-sql/react';
import {
  createShapeStreamOptions,
  type ElectricSharedTask,
} from '@/lib/electric';
import type { TaskStatus } from 'shared/types';

/**
 * Task shape from Electric SQL sync.
 * This is a simplified version of the full task type,
 * containing only the fields synced via Electric.
 */
export type ElectricTask = ElectricSharedTask;

/**
 * Result type for useElectricTasks hook.
 */
export interface UseElectricTasksResult {
  /** All tasks for the project, sorted by activity */
  tasks: ElectricTask[];

  /** Tasks indexed by ID for quick lookup */
  tasksById: Record<string, ElectricTask>;

  /** Tasks grouped by status for Kanban views */
  tasksByStatus: Record<TaskStatus, ElectricTask[]>;

  /** True while initial sync is in progress */
  isLoading: boolean;

  /** Error if sync failed */
  error: Error | null;

  /** True while actively syncing (including live updates) */
  isSyncing: boolean;
}

/**
 * Options for useElectricTasks hook.
 */
export interface UseElectricTasksOptions {
  /** Include archived tasks (default: false) */
  includeArchived?: boolean;
}

/**
 * Hook to sync tasks in real-time using Electric SQL.
 *
 * @param projectId - The project ID to filter tasks
 * @param options - Additional options
 * @returns Tasks data and sync state
 */
export function useElectricTasks(
  projectId: string | undefined,
  options: UseElectricTasksOptions = {}
): UseElectricTasksResult {
  const { includeArchived = false } = options;

  // Create shape stream options for shared_tasks table
  const shapeOptions = useMemo(() => {
    if (!projectId) {
      return null;
    }
    return createShapeStreamOptions('shared_tasks');
  }, [projectId]);

  // Subscribe to the Electric shape
  // The shape data is automatically synced from PostgreSQL
  const { data, isLoading, error } = useShape<ElectricSharedTask>(
    shapeOptions ?? {
      url: '',
    }
  );

  // Filter and process tasks
  const { tasks, tasksById, tasksByStatus } = useMemo(() => {
    // Return empty state if no project or no data
    if (!projectId || !data) {
      const emptyByStatus: Record<TaskStatus, ElectricTask[]> = {
        todo: [],
        inprogress: [],
        inreview: [],
        done: [],
        cancelled: [],
      };
      return {
        tasks: [],
        tasksById: {},
        tasksByStatus: emptyByStatus,
      };
    }

    // Filter tasks by project and deleted status
    const filtered = data.filter((task) => {
      // Filter by project - Electric syncs project_id from PostgreSQL
      // Both project_id (from Electric) and remote_project_id (compatibility) are checked
      const taskProjectId = task.project_id ?? task.remote_project_id;
      if (taskProjectId !== projectId) {
        return false;
      }

      // Exclude deleted tasks unless archived/deleted are requested
      // Check both deleted_at (PostgreSQL) and archived_at (compatibility)
      const isDeleted = task.deleted_at != null || task.archived_at != null;
      if (!includeArchived && isDeleted) {
        return false;
      }

      return true;
    });

    // Build tasks indexed by ID
    const byId: Record<string, ElectricTask> = {};
    for (const task of filtered) {
      byId[task.id] = task;
    }

    // Group tasks by status
    const byStatus: Record<TaskStatus, ElectricTask[]> = {
      todo: [],
      inprogress: [],
      inreview: [],
      done: [],
      cancelled: [],
    };

    for (const task of filtered) {
      if (task.status in byStatus) {
        byStatus[task.status as TaskStatus].push(task);
      }
    }

    // Sort tasks by activity time
    const getActivityTime = (task: ElectricTask): number => {
      const activityAt = task.activity_at ?? task.created_at;
      return new Date(activityAt as string | Date).getTime();
    };

    // Sort all tasks by activity (most recent first)
    const sorted = [...filtered].sort(
      (a, b) => getActivityTime(b) - getActivityTime(a)
    );

    // Apply status-aware sorting within each status group:
    // - Todo: oldest first (FIFO queue)
    // - All others: most recent first
    const statuses: TaskStatus[] = [
      'todo',
      'inprogress',
      'inreview',
      'done',
      'cancelled',
    ];
    for (const status of statuses) {
      if (status === 'todo') {
        byStatus[status].sort(
          (a, b) => getActivityTime(a) - getActivityTime(b)
        );
      } else {
        byStatus[status].sort(
          (a, b) => getActivityTime(b) - getActivityTime(a)
        );
      }
    }

    return {
      tasks: sorted,
      tasksById: byId,
      tasksByStatus: byStatus,
    };
  }, [data, projectId, includeArchived]);

  // Convert error to Error | null for consistent typing
  const normalizedError: Error | null =
    error == null
      ? null
      : error instanceof Error
        ? error
        : new Error(String(error));

  return {
    tasks,
    tasksById,
    tasksByStatus,
    isLoading: !projectId ? false : isLoading,
    error: normalizedError,
    isSyncing: isLoading,
  };
}
