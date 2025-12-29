/**
 * Tests for useElectricTasks hook
 *
 * This hook provides real-time task sync using Electric SQL.
 * TDD tests are written first, then implementation follows.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook } from '@testing-library/react';

// Mock data storage
type MockTask = {
  id: string;
  status: string;
  project_id: string;
  deleted_at?: string | null;
  created_at?: string;
  activity_at?: string | null;
};

let mockShapeData: MockTask[] = [];
let mockIsLoading = false;
let mockError: Error | null = null;

// Mock the Electric SQL React hook
vi.mock('@electric-sql/react', () => ({
  useShape: vi.fn(() => ({
    data: mockShapeData,
    isLoading: mockIsLoading,
    error: mockError,
  })),
}));

// Mock the Electric config
vi.mock('@/lib/electric', () => ({
  createShapeStreamOptions: vi.fn((table: string) => ({
    url: `/api/electric/v1/shape/${table}`,
  })),
}));

// Import after mocks are set up
import { useElectricTasks } from './useElectricTasks';

describe('useElectricTasks', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockShapeData = [];
    mockIsLoading = false;
    mockError = null;
  });

  describe('initial state', () => {
    it('returns empty array when no project is provided', () => {
      const { result } = renderHook(() => useElectricTasks(undefined));

      expect(result.current.tasks).toEqual([]);
      expect(result.current.isLoading).toBe(false);
    });

    it('returns empty array when project has no tasks', () => {
      const { result } = renderHook(() => useElectricTasks('project-1'));

      expect(result.current.tasks).toEqual([]);
    });
  });

  describe('data loading', () => {
    it('sets isLoading to true while syncing', () => {
      mockIsLoading = true;

      const { result } = renderHook(() => useElectricTasks('project-1'));

      expect(result.current.isLoading).toBe(true);
      expect(result.current.isSyncing).toBe(true);
    });

    it('sets isLoading to false when data is loaded', () => {
      mockIsLoading = false;
      mockShapeData = [
        {
          id: 'task-1',
          status: 'todo',
          project_id: 'project-1',
          deleted_at: null,
        },
      ];

      const { result } = renderHook(() => useElectricTasks('project-1'));

      expect(result.current.isLoading).toBe(false);
    });

    it('sets error when sync fails', () => {
      mockError = new Error('Connection failed');

      const { result } = renderHook(() => useElectricTasks('project-1'));

      expect(result.current.error).toEqual(expect.any(Error));
      expect(result.current.error?.message).toBe('Connection failed');
    });
  });

  describe('tasks filtering', () => {
    it('filters tasks by project_id', () => {
      mockShapeData = [
        {
          id: 'task-1',
          status: 'todo',
          project_id: 'project-1',
          deleted_at: null,
        },
        {
          id: 'task-2',
          status: 'inprogress',
          project_id: 'project-2',
          deleted_at: null,
        },
        {
          id: 'task-3',
          status: 'done',
          project_id: 'project-1',
          deleted_at: null,
        },
      ];

      const { result } = renderHook(() => useElectricTasks('project-1'));

      const taskIds = result.current.tasks.map((t) => t.id);
      expect(taskIds).toContain('task-1');
      expect(taskIds).toContain('task-3');
      expect(taskIds).not.toContain('task-2');
      expect(result.current.tasks.length).toBe(2);
    });

    it('excludes deleted tasks by default', () => {
      mockShapeData = [
        {
          id: 'task-1',
          status: 'todo',
          project_id: 'project-1',
          deleted_at: null,
        },
        {
          id: 'task-2',
          status: 'todo',
          project_id: 'project-1',
          deleted_at: '2024-01-01T00:00:00Z',
        },
      ];

      const { result } = renderHook(() => useElectricTasks('project-1'));

      const taskIds = result.current.tasks.map((t) => t.id);
      expect(taskIds).toContain('task-1');
      expect(taskIds).not.toContain('task-2');
      expect(result.current.tasks.length).toBe(1);
    });

    it('includes deleted tasks when includeArchived is true', () => {
      mockShapeData = [
        {
          id: 'task-1',
          status: 'todo',
          project_id: 'project-1',
          deleted_at: null,
        },
        {
          id: 'task-2',
          status: 'todo',
          project_id: 'project-1',
          deleted_at: '2024-01-01T00:00:00Z',
        },
      ];

      const { result } = renderHook(() =>
        useElectricTasks('project-1', { includeArchived: true })
      );

      const taskIds = result.current.tasks.map((t) => t.id);
      expect(taskIds).toContain('task-1');
      expect(taskIds).toContain('task-2');
      expect(result.current.tasks.length).toBe(2);
    });
  });

  describe('tasksById', () => {
    it('provides tasks indexed by id', () => {
      mockShapeData = [
        {
          id: 'task-1',
          status: 'todo',
          project_id: 'project-1',
          deleted_at: null,
        },
        {
          id: 'task-2',
          status: 'done',
          project_id: 'project-1',
          deleted_at: null,
        },
      ];

      const { result } = renderHook(() => useElectricTasks('project-1'));

      expect(result.current.tasksById['task-1']).toBeDefined();
      expect(result.current.tasksById['task-2']).toBeDefined();
      expect(result.current.tasksById['task-1'].status).toBe('todo');
      expect(result.current.tasksById['task-2'].status).toBe('done');
    });

    it('returns empty object when no tasks', () => {
      const { result } = renderHook(() => useElectricTasks('project-1'));

      expect(result.current.tasksById).toEqual({});
    });
  });

  describe('tasksByStatus', () => {
    it('groups tasks by status', () => {
      mockShapeData = [
        {
          id: 'task-1',
          status: 'todo',
          project_id: 'project-1',
          deleted_at: null,
        },
        {
          id: 'task-2',
          status: 'todo',
          project_id: 'project-1',
          deleted_at: null,
        },
        {
          id: 'task-3',
          status: 'inprogress',
          project_id: 'project-1',
          deleted_at: null,
        },
        {
          id: 'task-4',
          status: 'done',
          project_id: 'project-1',
          deleted_at: null,
        },
      ];

      const { result } = renderHook(() => useElectricTasks('project-1'));

      expect(result.current.tasksByStatus.todo.length).toBe(2);
      expect(result.current.tasksByStatus.inprogress.length).toBe(1);
      expect(result.current.tasksByStatus.done.length).toBe(1);
      expect(result.current.tasksByStatus.inreview.length).toBe(0);
      expect(result.current.tasksByStatus.cancelled.length).toBe(0);
    });

    it('has all status categories even when empty', () => {
      const { result } = renderHook(() => useElectricTasks('project-1'));

      expect(result.current.tasksByStatus).toHaveProperty('todo');
      expect(result.current.tasksByStatus).toHaveProperty('inprogress');
      expect(result.current.tasksByStatus).toHaveProperty('inreview');
      expect(result.current.tasksByStatus).toHaveProperty('done');
      expect(result.current.tasksByStatus).toHaveProperty('cancelled');
    });
  });

  describe('sorting', () => {
    it('sorts todo tasks by oldest first (FIFO)', () => {
      mockShapeData = [
        {
          id: 'task-newer',
          status: 'todo',
          project_id: 'project-1',
          deleted_at: null,
          created_at: '2024-01-02T00:00:00Z',
          activity_at: '2024-01-02T00:00:00Z',
        },
        {
          id: 'task-older',
          status: 'todo',
          project_id: 'project-1',
          deleted_at: null,
          created_at: '2024-01-01T00:00:00Z',
          activity_at: '2024-01-01T00:00:00Z',
        },
      ];

      const { result } = renderHook(() => useElectricTasks('project-1'));

      // Todo tasks should be oldest first
      expect(result.current.tasksByStatus.todo[0].id).toBe('task-older');
      expect(result.current.tasksByStatus.todo[1].id).toBe('task-newer');
    });

    it('sorts non-todo tasks by most recent first', () => {
      mockShapeData = [
        {
          id: 'task-older',
          status: 'done',
          project_id: 'project-1',
          deleted_at: null,
          created_at: '2024-01-01T00:00:00Z',
          activity_at: '2024-01-01T00:00:00Z',
        },
        {
          id: 'task-newer',
          status: 'done',
          project_id: 'project-1',
          deleted_at: null,
          created_at: '2024-01-02T00:00:00Z',
          activity_at: '2024-01-02T00:00:00Z',
        },
      ];

      const { result } = renderHook(() => useElectricTasks('project-1'));

      // Done tasks should be most recent first
      expect(result.current.tasksByStatus.done[0].id).toBe('task-newer');
      expect(result.current.tasksByStatus.done[1].id).toBe('task-older');
    });
  });

  describe('result shape', () => {
    it('returns all expected properties', () => {
      const { result } = renderHook(() => useElectricTasks('project-1'));

      expect(result.current).toHaveProperty('tasks');
      expect(result.current).toHaveProperty('tasksById');
      expect(result.current).toHaveProperty('tasksByStatus');
      expect(result.current).toHaveProperty('isLoading');
      expect(result.current).toHaveProperty('error');
      expect(result.current).toHaveProperty('isSyncing');
    });

    it('isSyncing reflects Electric sync state', () => {
      mockIsLoading = true;

      const { result } = renderHook(() => useElectricTasks('project-1'));

      expect(result.current.isSyncing).toBe(true);
    });
  });
});
