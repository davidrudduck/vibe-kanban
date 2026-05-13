import { renderHook, act } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';
import { useDeferredMount } from '@/shared/hooks/useDeferredMount';

describe('useDeferredMount', () => {
  it('returns false on initial render', () => {
    const { result } = renderHook(() => useDeferredMount());
    expect(result.current).toBe(false);
  });

  it('flips to true after the deferral completes', async () => {
    vi.useFakeTimers();
    const { result } = renderHook(() => useDeferredMount());
    expect(result.current).toBe(false);
    await act(async () => {
      vi.runAllTimers();
    });
    expect(result.current).toBe(true);
    vi.useRealTimers();
  });
});
