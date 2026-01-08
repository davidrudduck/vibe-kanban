import { describe, it, expect, beforeEach } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { useColumnSortState } from './useColumnSortState';

const STORAGE_KEY = 'kanban-sort-directions-v1';

describe('useColumnSortState', () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it('loads default directions when localStorage is empty', () => {
    const { result } = renderHook(() => useColumnSortState());
    expect(result.current.sortDirections.todo).toBe('asc');
    expect(result.current.sortDirections.inprogress).toBe('asc');
    expect(result.current.sortDirections.inreview).toBe('asc');
    expect(result.current.sortDirections.done).toBe('asc');
    expect(result.current.sortDirections.cancelled).toBe('asc');
  });

  it('persists sort direction changes to localStorage', () => {
    const { result } = renderHook(() => useColumnSortState());

    act(() => {
      result.current.toggleDirection('todo');
    });

    expect(result.current.sortDirections.todo).toBe('desc');

    const stored = JSON.parse(localStorage.getItem(STORAGE_KEY) || '{}');
    expect(stored.todo).toBe('desc');
  });

  it('loads persisted directions from localStorage', () => {
    localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify({
        todo: 'desc',
        inprogress: 'asc',
        inreview: 'desc',
        done: 'asc',
        cancelled: 'asc',
      })
    );

    const { result } = renderHook(() => useColumnSortState());
    expect(result.current.sortDirections.todo).toBe('desc');
    expect(result.current.sortDirections.inreview).toBe('desc');
    expect(result.current.sortDirections.inprogress).toBe('asc');
  });

  it('falls back to defaults when localStorage has invalid JSON', () => {
    localStorage.setItem(STORAGE_KEY, 'invalid json');

    const { result } = renderHook(() => useColumnSortState());
    expect(result.current.sortDirections.todo).toBe('asc');
    expect(result.current.sortDirections.done).toBe('asc');
  });

  it('falls back to defaults when localStorage has invalid direction values', () => {
    localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify({
        todo: 'invalid',
        inprogress: 'asc',
        inreview: 'asc',
        done: 'asc',
        cancelled: 'asc',
      })
    );

    const { result } = renderHook(() => useColumnSortState());
    // Should fall back to defaults because 'invalid' is not a valid direction
    expect(result.current.sortDirections.todo).toBe('asc');
  });

  it('falls back to defaults when localStorage is missing required statuses', () => {
    localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify({
        todo: 'desc',
        // Missing other statuses
      })
    );

    const { result } = renderHook(() => useColumnSortState());
    // Should fall back to defaults because not all statuses are present
    expect(result.current.sortDirections.todo).toBe('asc');
  });

  it('resetDirections clears persisted values and restores defaults', () => {
    const { result } = renderHook(() => useColumnSortState());

    act(() => {
      result.current.toggleDirection('todo');
      result.current.toggleDirection('done');
    });

    expect(result.current.sortDirections.todo).toBe('desc');
    expect(result.current.sortDirections.done).toBe('desc');

    act(() => {
      result.current.resetDirections();
    });

    expect(result.current.sortDirections.todo).toBe('asc');
    expect(result.current.sortDirections.done).toBe('asc');

    const stored = JSON.parse(localStorage.getItem(STORAGE_KEY) || '{}');
    expect(stored.todo).toBe('asc');
    expect(stored.done).toBe('asc');
  });

  it('toggleDirection updates only the specified column', () => {
    const { result } = renderHook(() => useColumnSortState());

    act(() => {
      result.current.toggleDirection('inprogress');
    });

    expect(result.current.sortDirections.todo).toBe('asc');
    expect(result.current.sortDirections.inprogress).toBe('desc');
    expect(result.current.sortDirections.inreview).toBe('asc');
    expect(result.current.sortDirections.done).toBe('asc');
    expect(result.current.sortDirections.cancelled).toBe('asc');
  });

  it('multiple toggles cycle between asc and desc', () => {
    const { result } = renderHook(() => useColumnSortState());

    expect(result.current.sortDirections.todo).toBe('asc');

    act(() => {
      result.current.toggleDirection('todo');
    });
    expect(result.current.sortDirections.todo).toBe('desc');

    act(() => {
      result.current.toggleDirection('todo');
    });
    expect(result.current.sortDirections.todo).toBe('asc');

    act(() => {
      result.current.toggleDirection('todo');
    });
    expect(result.current.sortDirections.todo).toBe('desc');
  });
});
