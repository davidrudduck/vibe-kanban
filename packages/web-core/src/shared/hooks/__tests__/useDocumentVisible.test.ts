import { renderHook, act } from '@testing-library/react';
import { describe, it, expect, afterEach, vi } from 'vitest';
import { useDocumentVisible } from '@/shared/hooks/useDocumentVisible';

function setHidden(hidden: boolean) {
  Object.defineProperty(document, 'visibilityState', {
    configurable: true,
    get: () => (hidden ? 'hidden' : 'visible'),
  });
  Object.defineProperty(document, 'hidden', {
    configurable: true,
    get: () => hidden,
  });
  document.dispatchEvent(new Event('visibilitychange'));
}

afterEach(() => {
  vi.useRealTimers();
  setHidden(false);
});

describe('useDocumentVisible', () => {
  it('returns true initially when document is visible', () => {
    setHidden(false);
    const { result } = renderHook(() => useDocumentVisible(30_000));
    expect(result.current).toBe(true);
  });

  it('stays true during the grace period when hidden briefly', () => {
    vi.useFakeTimers();
    setHidden(false);
    const { result } = renderHook(() => useDocumentVisible(30_000));

    act(() => setHidden(true));
    act(() => {
      vi.advanceTimersByTime(15_000);
    });
    expect(result.current).toBe(true);

    act(() => setHidden(false));
    act(() => {
      vi.advanceTimersByTime(30_000);
    });
    expect(result.current).toBe(true);
  });

  it('flips to false after the grace period elapses while hidden', () => {
    vi.useFakeTimers();
    setHidden(false);
    const { result } = renderHook(() => useDocumentVisible(30_000));

    act(() => setHidden(true));
    act(() => {
      vi.advanceTimersByTime(30_001);
    });
    expect(result.current).toBe(false);
  });

  it('flips back to true immediately when document becomes visible again', () => {
    vi.useFakeTimers();
    setHidden(false);
    const { result } = renderHook(() => useDocumentVisible(30_000));

    act(() => setHidden(true));
    act(() => {
      vi.advanceTimersByTime(30_001);
    });
    expect(result.current).toBe(false);

    act(() => setHidden(false));
    expect(result.current).toBe(true);
  });
});
