import type { TaskStatus } from 'shared/types';

/**
 * Sort direction for task ordering
 */
export type SortDirection = 'asc' | 'desc';

/**
 * Minimum interface for tasks that can be sorted.
 * This allows sorting functions to work with both TaskWithAttemptStatus
 * and TaskWithProjectInfo types.
 */
export interface SortableTask {
  status: TaskStatus;
  created_at: string | Date;
  activity_at?: string | Date | null;
  latest_execution_started_at?: string | Date | null;
  latest_execution_completed_at?: string | Date | null;
}

/**
 * Converts a date value (string, Date, or null) to a Unix timestamp.
 * Returns 0 for null/undefined values.
 */
function toTimestamp(value: string | Date | null | undefined): number {
  if (value == null) {
    return 0;
  }
  return new Date(value).getTime();
}

/**
 * Gets the appropriate sort timestamp for a task based on its status.
 *
 * Sorting strategy:
 * - **Todo**: Use `created_at` (FIFO queue - oldest first). This is the key bug fix -
 *   previously used `activity_at ?? created_at` which caused incorrect ordering.
 * - **In Progress**: Use `latest_execution_started_at` to show longest-running tasks first.
 * - **In Review/Done/Cancelled**: Use `latest_execution_completed_at` to show oldest
 *   completed tasks first for review or archive purposes.
 *
 * All statuses fall back to `created_at` when the preferred timestamp is null.
 *
 * @param task - The task to get a sort timestamp for
 * @returns Unix timestamp (milliseconds since epoch)
 */
export function getSortTimestamp(task: SortableTask): number {
  const createdAt = toTimestamp(task.created_at);

  switch (task.status) {
    case 'todo':
      // Todo tasks: always use created_at for FIFO behavior
      // Intentionally ignoring activity_at to fix the sorting bug
      return createdAt;

    case 'inprogress':
      // In-progress tasks: use execution start time, fall back to created_at
      return toTimestamp(task.latest_execution_started_at) || createdAt;

    case 'inreview':
    case 'done':
    case 'cancelled':
      // Completed/review tasks: use execution completion time, fall back to created_at
      return toTimestamp(task.latest_execution_completed_at) || createdAt;

    default:
      // Unknown status: use created_at
      return createdAt;
  }
}

/**
 * Sorts an array of tasks by their status-appropriate timestamp.
 *
 * @param tasks - Array of tasks to sort (must all have the same status for meaningful results)
 * @param direction - Sort direction: 'asc' (oldest first, default) or 'desc' (newest first)
 * @returns New sorted array (original is not mutated)
 */
export function sortTasksByStatus<T extends SortableTask>(
  tasks: T[],
  direction: SortDirection = 'asc'
): T[] {
  const multiplier = direction === 'asc' ? 1 : -1;

  return [...tasks].sort((a, b) => {
    const aTime = getSortTimestamp(a);
    const bTime = getSortTimestamp(b);
    return (aTime - bTime) * multiplier;
  });
}

/**
 * Sorts all task groups (organized by status) with configurable directions per status.
 *
 * @param tasksByStatus - Record mapping TaskStatus to arrays of tasks
 * @param directions - Optional partial record of sort directions per status.
 *                     Defaults to 'asc' (oldest first) for any status not specified.
 * @returns New object with sorted arrays (original is not mutated)
 */
export function sortTaskGroups<T extends SortableTask>(
  tasksByStatus: Record<TaskStatus, T[]>,
  directions?: Partial<Record<TaskStatus, SortDirection>>
): Record<TaskStatus, T[]> {
  const statuses: TaskStatus[] = [
    'todo',
    'inprogress',
    'inreview',
    'done',
    'cancelled',
  ];

  const result = {} as Record<TaskStatus, T[]>;

  for (const status of statuses) {
    const tasks = tasksByStatus[status] || [];
    const direction = directions?.[status] ?? 'asc';
    result[status] = sortTasksByStatus(tasks, direction);
  }

  return result;
}
