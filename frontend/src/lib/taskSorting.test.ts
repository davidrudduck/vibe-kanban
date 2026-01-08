import { describe, it, expect } from 'vitest';
import type { TaskStatus } from 'shared/types';
import {
  getSortTimestamp,
  sortTasksByStatus,
  sortTaskGroups,
  type SortableTask,
  type SortDirection,
} from './taskSorting';

// Helper to create mock tasks
function createMockTask(
  overrides: Partial<SortableTask> & { status: TaskStatus }
): SortableTask {
  return {
    created_at: '2024-01-01T00:00:00Z',
    activity_at: null,
    latest_execution_started_at: null,
    latest_execution_completed_at: null,
    ...overrides,
  };
}

describe('getSortTimestamp', () => {
  it('uses created_at for todo tasks, ignoring activity_at', () => {
    const task = createMockTask({
      status: 'todo',
      created_at: '2024-01-01T00:00:00Z',
      activity_at: '2024-06-15T12:00:00Z', // Should be ignored
    });

    const timestamp = getSortTimestamp(task);

    // Should use created_at, not activity_at
    expect(timestamp).toBe(new Date('2024-01-01T00:00:00Z').getTime());
  });

  it('uses latest_execution_started_at for inprogress tasks', () => {
    const task = createMockTask({
      status: 'inprogress',
      created_at: '2024-01-01T00:00:00Z',
      latest_execution_started_at: '2024-06-15T12:00:00Z',
    });

    const timestamp = getSortTimestamp(task);

    expect(timestamp).toBe(new Date('2024-06-15T12:00:00Z').getTime());
  });

  it('uses latest_execution_completed_at for done tasks', () => {
    const task = createMockTask({
      status: 'done',
      created_at: '2024-01-01T00:00:00Z',
      latest_execution_completed_at: '2024-06-20T15:00:00Z',
    });

    const timestamp = getSortTimestamp(task);

    expect(timestamp).toBe(new Date('2024-06-20T15:00:00Z').getTime());
  });

  it('uses latest_execution_completed_at for inreview tasks', () => {
    const task = createMockTask({
      status: 'inreview',
      created_at: '2024-01-01T00:00:00Z',
      latest_execution_completed_at: '2024-06-18T10:00:00Z',
    });

    const timestamp = getSortTimestamp(task);

    expect(timestamp).toBe(new Date('2024-06-18T10:00:00Z').getTime());
  });

  it('uses latest_execution_completed_at for cancelled tasks', () => {
    const task = createMockTask({
      status: 'cancelled',
      created_at: '2024-01-01T00:00:00Z',
      latest_execution_completed_at: '2024-06-25T08:00:00Z',
    });

    const timestamp = getSortTimestamp(task);

    expect(timestamp).toBe(new Date('2024-06-25T08:00:00Z').getTime());
  });

  it('falls back to created_at when latest_execution_started_at is null for inprogress', () => {
    const task = createMockTask({
      status: 'inprogress',
      created_at: '2024-01-01T00:00:00Z',
      latest_execution_started_at: null,
    });

    const timestamp = getSortTimestamp(task);

    expect(timestamp).toBe(new Date('2024-01-01T00:00:00Z').getTime());
  });

  it('falls back to created_at when latest_execution_completed_at is null for done', () => {
    const task = createMockTask({
      status: 'done',
      created_at: '2024-01-01T00:00:00Z',
      latest_execution_completed_at: null,
    });

    const timestamp = getSortTimestamp(task);

    expect(timestamp).toBe(new Date('2024-01-01T00:00:00Z').getTime());
  });

  it('handles Date objects in addition to strings', () => {
    const task = createMockTask({
      status: 'todo',
      created_at: new Date('2024-01-01T00:00:00Z'),
    });

    const timestamp = getSortTimestamp(task);

    expect(timestamp).toBe(new Date('2024-01-01T00:00:00Z').getTime());
  });
});

describe('sortTasksByStatus', () => {
  it('sorts todo tasks oldest first by default (ASC by created_at)', () => {
    const tasks = [
      createMockTask({ status: 'todo', created_at: '2024-03-01T00:00:00Z' }),
      createMockTask({ status: 'todo', created_at: '2024-01-01T00:00:00Z' }),
      createMockTask({ status: 'todo', created_at: '2024-02-01T00:00:00Z' }),
    ];

    const sorted = sortTasksByStatus(tasks);

    expect(sorted[0].created_at).toBe('2024-01-01T00:00:00Z');
    expect(sorted[1].created_at).toBe('2024-02-01T00:00:00Z');
    expect(sorted[2].created_at).toBe('2024-03-01T00:00:00Z');
  });

  it('sorts inprogress tasks oldest first by default (ASC by latest_execution_started_at)', () => {
    const tasks = [
      createMockTask({
        status: 'inprogress',
        created_at: '2024-01-01T00:00:00Z',
        latest_execution_started_at: '2024-03-01T00:00:00Z',
      }),
      createMockTask({
        status: 'inprogress',
        created_at: '2024-01-02T00:00:00Z',
        latest_execution_started_at: '2024-01-15T00:00:00Z',
      }),
      createMockTask({
        status: 'inprogress',
        created_at: '2024-01-03T00:00:00Z',
        latest_execution_started_at: '2024-02-01T00:00:00Z',
      }),
    ];

    const sorted = sortTasksByStatus(tasks);

    // Sorted by latest_execution_started_at, oldest first
    expect(sorted[0].latest_execution_started_at).toBe('2024-01-15T00:00:00Z');
    expect(sorted[1].latest_execution_started_at).toBe('2024-02-01T00:00:00Z');
    expect(sorted[2].latest_execution_started_at).toBe('2024-03-01T00:00:00Z');
  });

  it('reverses order when direction is DESC', () => {
    const tasks = [
      createMockTask({ status: 'todo', created_at: '2024-03-01T00:00:00Z' }),
      createMockTask({ status: 'todo', created_at: '2024-01-01T00:00:00Z' }),
      createMockTask({ status: 'todo', created_at: '2024-02-01T00:00:00Z' }),
    ];

    const sorted = sortTasksByStatus(tasks, 'desc');

    // Newest first
    expect(sorted[0].created_at).toBe('2024-03-01T00:00:00Z');
    expect(sorted[1].created_at).toBe('2024-02-01T00:00:00Z');
    expect(sorted[2].created_at).toBe('2024-01-01T00:00:00Z');
  });

  it('does not mutate original array', () => {
    const tasks = [
      createMockTask({ status: 'todo', created_at: '2024-03-01T00:00:00Z' }),
      createMockTask({ status: 'todo', created_at: '2024-01-01T00:00:00Z' }),
    ];
    const originalFirstTask = tasks[0];

    const sorted = sortTasksByStatus(tasks);

    // Original array unchanged
    expect(tasks[0]).toBe(originalFirstTask);
    expect(tasks[0].created_at).toBe('2024-03-01T00:00:00Z');

    // Sorted array is different reference
    expect(sorted).not.toBe(tasks);
    expect(sorted[0].created_at).toBe('2024-01-01T00:00:00Z');
  });

  it('handles empty array', () => {
    const sorted = sortTasksByStatus([]);

    expect(sorted).toEqual([]);
  });

  it('handles single element array', () => {
    const tasks = [
      createMockTask({ status: 'todo', created_at: '2024-01-01T00:00:00Z' }),
    ];

    const sorted = sortTasksByStatus(tasks);

    expect(sorted).toHaveLength(1);
    expect(sorted[0].created_at).toBe('2024-01-01T00:00:00Z');
  });

  it('handles tasks with same timestamps', () => {
    const tasks = [
      createMockTask({ status: 'todo', created_at: '2024-01-01T00:00:00Z' }),
      createMockTask({ status: 'todo', created_at: '2024-01-01T00:00:00Z' }),
    ];

    const sorted = sortTasksByStatus(tasks);

    expect(sorted).toHaveLength(2);
    // Both should have same timestamp, order is stable
    expect(sorted[0].created_at).toBe('2024-01-01T00:00:00Z');
    expect(sorted[1].created_at).toBe('2024-01-01T00:00:00Z');
  });
});

describe('sortTaskGroups', () => {
  it('sorts all status groups correctly with default ASC', () => {
    const tasksByStatus: Record<TaskStatus, SortableTask[]> = {
      todo: [
        createMockTask({ status: 'todo', created_at: '2024-03-01T00:00:00Z' }),
        createMockTask({ status: 'todo', created_at: '2024-01-01T00:00:00Z' }),
      ],
      inprogress: [
        createMockTask({
          status: 'inprogress',
          created_at: '2024-01-01T00:00:00Z',
          latest_execution_started_at: '2024-03-01T00:00:00Z',
        }),
        createMockTask({
          status: 'inprogress',
          created_at: '2024-01-02T00:00:00Z',
          latest_execution_started_at: '2024-01-15T00:00:00Z',
        }),
      ],
      inreview: [
        createMockTask({
          status: 'inreview',
          created_at: '2024-01-01T00:00:00Z',
          latest_execution_completed_at: '2024-02-15T00:00:00Z',
        }),
        createMockTask({
          status: 'inreview',
          created_at: '2024-01-02T00:00:00Z',
          latest_execution_completed_at: '2024-01-20T00:00:00Z',
        }),
      ],
      done: [],
      cancelled: [],
    };

    const sorted = sortTaskGroups(tasksByStatus);

    // Todo: oldest created_at first
    expect(sorted.todo[0].created_at).toBe('2024-01-01T00:00:00Z');
    expect(sorted.todo[1].created_at).toBe('2024-03-01T00:00:00Z');

    // Inprogress: oldest latest_execution_started_at first
    expect(sorted.inprogress[0].latest_execution_started_at).toBe(
      '2024-01-15T00:00:00Z'
    );
    expect(sorted.inprogress[1].latest_execution_started_at).toBe(
      '2024-03-01T00:00:00Z'
    );

    // Inreview: oldest latest_execution_completed_at first
    expect(sorted.inreview[0].latest_execution_completed_at).toBe(
      '2024-01-20T00:00:00Z'
    );
    expect(sorted.inreview[1].latest_execution_completed_at).toBe(
      '2024-02-15T00:00:00Z'
    );
  });

  it('applies custom directions per status', () => {
    const tasksByStatus: Record<TaskStatus, SortableTask[]> = {
      todo: [
        createMockTask({ status: 'todo', created_at: '2024-03-01T00:00:00Z' }),
        createMockTask({ status: 'todo', created_at: '2024-01-01T00:00:00Z' }),
      ],
      inprogress: [],
      inreview: [],
      done: [
        createMockTask({
          status: 'done',
          created_at: '2024-01-01T00:00:00Z',
          latest_execution_completed_at: '2024-02-01T00:00:00Z',
        }),
        createMockTask({
          status: 'done',
          created_at: '2024-01-02T00:00:00Z',
          latest_execution_completed_at: '2024-03-01T00:00:00Z',
        }),
      ],
      cancelled: [],
    };

    const directions: Partial<Record<TaskStatus, SortDirection>> = {
      todo: 'desc', // Newest first
      done: 'desc', // Newest completed first
    };

    const sorted = sortTaskGroups(tasksByStatus, directions);

    // Todo: DESC means newest first
    expect(sorted.todo[0].created_at).toBe('2024-03-01T00:00:00Z');
    expect(sorted.todo[1].created_at).toBe('2024-01-01T00:00:00Z');

    // Done: DESC means newest completed first
    expect(sorted.done[0].latest_execution_completed_at).toBe(
      '2024-03-01T00:00:00Z'
    );
    expect(sorted.done[1].latest_execution_completed_at).toBe(
      '2024-02-01T00:00:00Z'
    );
  });

  it('handles empty groups', () => {
    const tasksByStatus: Record<TaskStatus, SortableTask[]> = {
      todo: [],
      inprogress: [],
      inreview: [],
      done: [],
      cancelled: [],
    };

    const sorted = sortTaskGroups(tasksByStatus);

    expect(sorted.todo).toEqual([]);
    expect(sorted.inprogress).toEqual([]);
    expect(sorted.inreview).toEqual([]);
    expect(sorted.done).toEqual([]);
    expect(sorted.cancelled).toEqual([]);
  });

  it('does not mutate original groups', () => {
    const tasksByStatus: Record<TaskStatus, SortableTask[]> = {
      todo: [
        createMockTask({ status: 'todo', created_at: '2024-03-01T00:00:00Z' }),
        createMockTask({ status: 'todo', created_at: '2024-01-01T00:00:00Z' }),
      ],
      inprogress: [],
      inreview: [],
      done: [],
      cancelled: [],
    };
    const originalFirstTodo = tasksByStatus.todo[0];

    const sorted = sortTaskGroups(tasksByStatus);

    // Original array unchanged
    expect(tasksByStatus.todo[0]).toBe(originalFirstTodo);
    expect(tasksByStatus.todo[0].created_at).toBe('2024-03-01T00:00:00Z');

    // Sorted result is different
    expect(sorted.todo[0].created_at).toBe('2024-01-01T00:00:00Z');
  });
});
