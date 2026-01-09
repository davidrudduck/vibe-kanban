import { useState, useCallback, useEffect } from 'react';
import type { TaskStatus } from 'shared/types';
import type { SortDirection } from '@/lib/taskSorting';

/**
 * localStorage key for persisting sort directions.
 * Versioned to allow future migrations if the storage format changes.
 */
const STORAGE_KEY = 'kanban-sort-directions-v1';

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
 * All valid task statuses.
 */
const STATUSES: TaskStatus[] = [
  'todo',
  'inprogress',
  'inreview',
  'done',
  'cancelled',
];

/**
 * Valid sort direction values.
 */
const VALID_DIRECTIONS: SortDirection[] = ['asc', 'desc'];

/**
 * Loads sort directions from localStorage.
 * Falls back to defaults if localStorage is empty, invalid, or has malformed data.
 */
function loadSortDirections(): Record<TaskStatus, SortDirection> {
  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    if (stored) {
      const parsed = JSON.parse(stored);

      // Validate that parsed value is an object
      if (typeof parsed !== 'object' || parsed === null) {
        return DEFAULT_SORT_DIRECTIONS;
      }

      // Validate that all statuses exist and have valid direction values
      const isValid = STATUSES.every(
        (status) =>
          status in parsed &&
          VALID_DIRECTIONS.includes(parsed[status] as SortDirection)
      );

      if (isValid) {
        return parsed as Record<TaskStatus, SortDirection>;
      }
    }
  } catch (e) {
    // JSON.parse failed or localStorage not available
    console.warn('Failed to load sort directions from localStorage:', e);
  }
  return DEFAULT_SORT_DIRECTIONS;
}

/**
 * Saves sort directions to localStorage.
 * Silently catches errors (e.g., localStorage disabled, quota exceeded).
 */
function saveSortDirections(
  directions: Record<TaskStatus, SortDirection>
): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(directions));
  } catch (e) {
    console.warn('Failed to save sort directions to localStorage:', e);
  }
}

/**
 * Hook to manage sort directions for kanban board columns.
 *
 * Each column can be independently toggled between ascending (oldest first)
 * and descending (newest first) sort order.
 *
 * Sort preferences are persisted to localStorage and restored on page reload.
 *
 * @returns An object containing:
 *   - sortDirections: Current sort direction for each status column
 *   - toggleDirection: Function to toggle sort direction for a specific status
 *   - resetDirections: Function to reset all directions to defaults
 */
export function useColumnSortState() {
  const [sortDirections, setSortDirections] = useState<
    Record<TaskStatus, SortDirection>
  >(() => loadSortDirections());

  // Persist to localStorage whenever sort directions change
  useEffect(() => {
    saveSortDirections(sortDirections);
  }, [sortDirections]);

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
