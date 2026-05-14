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

  it('passes { timeout: 500 } to requestIdleCallback', () => {
    let capturedOptions: { timeout?: number } | undefined;
    const mockRic = vi.fn((cb: () => void, options?: { timeout?: number }) => {
      capturedOptions = options;
      cb();
      return 1;
    });
    const mockCancelRic = vi.fn();

    Object.assign(globalThis, {
      requestIdleCallback: mockRic,
      cancelIdleCallback: mockCancelRic,
    });

    renderHook(() => useDeferredMount());

    expect(mockRic).toHaveBeenCalledOnce();
    expect(capturedOptions).toEqual({ timeout: 500 });

    delete (globalThis as Record<string, unknown>).requestIdleCallback;
    delete (globalThis as Record<string, unknown>).cancelIdleCallback;
  });
});
