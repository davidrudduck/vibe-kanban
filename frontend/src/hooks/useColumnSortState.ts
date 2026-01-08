import { useState, useCallback } from 'react';
import type { TaskStatus } from 'shared/types';
import type { SortDirection } from '@/lib/taskSorting';

/**
 * Default sort directions for each task status column.
 * All columns default to 'asc' (oldest first).
 */
const DEFAULT_SORT_DIRECTIONS: Record<TaskStatus, SortDirection> = {
  todo: 'asc',
  inprogress: 'asc',
  inreview: 'asc',
  done: 'asc',
  cancelled: 'asc',
};

/**
 * Hook to manage sort directions for kanban board columns.
 *
 * Each column can be independently toggled between ascending (oldest first)
 * and descending (newest first) sort order.
 *
 * @returns An object containing:
 *   - sortDirections: Current sort direction for each status column
 *   - toggleDirection: Function to toggle sort direction for a specific status
 *   - resetDirections: Function to reset all directions to defaults
 */
export function useColumnSortState() {
  const [sortDirections, setSortDirections] =
    useState<Record<TaskStatus, SortDirection>>(DEFAULT_SORT_DIRECTIONS);

  const toggleDirection = useCallback((status: TaskStatus) => {
    setSortDirections((prev) => ({
      ...prev,
      [status]: prev[status] === 'asc' ? 'desc' : 'asc',
    }));
  }, []);

  const resetDirections = useCallback(() => {
    setSortDirections(DEFAULT_SORT_DIRECTIONS);
  }, []);

  return {
    sortDirections,
    toggleDirection,
    resetDirections,
  };
}
