import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import type { TouchEvent as ReactTouchEvent } from 'react';
import { useSwipe } from './useSwipe';

// Helper to create mock touch events
function createTouchEvent(clientX: number, clientY: number): ReactTouchEvent {
  return {
    touches: [{ clientX, clientY }],
    changedTouches: [{ clientX, clientY }],
  } as unknown as ReactTouchEvent;
}

describe('useSwipe', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  describe('swipe detection', () => {
    it('calls onSwipeLeft for left swipe', () => {
      const onSwipeLeft = vi.fn();
      const onSwipeRight = vi.fn();

      const { result } = renderHook(() =>
        useSwipe({ onSwipeLeft, onSwipeRight })
      );

      // Start touch at x=200
      act(() => {
        result.current.onTouchStart(createTouchEvent(200, 100));
      });

      // End touch at x=100 (moved left by 100px)
      act(() => {
        result.current.onTouchEnd(createTouchEvent(100, 100));
      });

      expect(onSwipeLeft).toHaveBeenCalledTimes(1);
      expect(onSwipeRight).not.toHaveBeenCalled();
    });

    it('calls onSwipeRight for right swipe', () => {
      const onSwipeLeft = vi.fn();
      const onSwipeRight = vi.fn();

      const { result } = renderHook(() =>
        useSwipe({ onSwipeLeft, onSwipeRight })
      );

      // Start touch at x=100
      act(() => {
        result.current.onTouchStart(createTouchEvent(100, 100));
      });

      // End touch at x=200 (moved right by 100px)
      act(() => {
        result.current.onTouchEnd(createTouchEvent(200, 100));
      });

      expect(onSwipeRight).toHaveBeenCalledTimes(1);
      expect(onSwipeLeft).not.toHaveBeenCalled();
    });

    it('does not trigger for small movements (below threshold)', () => {
      const onSwipeLeft = vi.fn();
      const onSwipeRight = vi.fn();

      const { result } = renderHook(() =>
        useSwipe({ onSwipeLeft, onSwipeRight }, { threshold: 50 })
      );

      // Start touch
      act(() => {
        result.current.onTouchStart(createTouchEvent(100, 100));
      });

      // End touch with only 30px movement (below 50px threshold)
      act(() => {
        result.current.onTouchEnd(createTouchEvent(130, 100));
      });

      expect(onSwipeLeft).not.toHaveBeenCalled();
      expect(onSwipeRight).not.toHaveBeenCalled();
    });

    it('does not trigger for vertical swipes', () => {
      const onSwipeLeft = vi.fn();
      const onSwipeRight = vi.fn();

      const { result } = renderHook(() =>
        useSwipe({ onSwipeLeft, onSwipeRight })
      );

      // Start touch
      act(() => {
        result.current.onTouchStart(createTouchEvent(100, 100));
      });

      // End touch with more vertical than horizontal movement
      act(() => {
        result.current.onTouchEnd(createTouchEvent(120, 300));
      });

      expect(onSwipeLeft).not.toHaveBeenCalled();
      expect(onSwipeRight).not.toHaveBeenCalled();
    });

    it('does not trigger for slow swipes (exceeds maxTime)', () => {
      const onSwipeLeft = vi.fn();
      const onSwipeRight = vi.fn();

      const { result } = renderHook(() =>
        useSwipe({ onSwipeLeft, onSwipeRight }, { maxTime: 300 })
      );

      // Start touch
      act(() => {
        result.current.onTouchStart(createTouchEvent(200, 100));
      });

      // Advance time beyond maxTime
      act(() => {
        vi.advanceTimersByTime(400);
      });

      // End touch (should not trigger because too slow)
      act(() => {
        result.current.onTouchEnd(createTouchEvent(100, 100));
      });

      expect(onSwipeLeft).not.toHaveBeenCalled();
      expect(onSwipeRight).not.toHaveBeenCalled();
    });
  });

  describe('edge cases', () => {
    it('handles missing touch start', () => {
      const onSwipeLeft = vi.fn();

      const { result } = renderHook(() => useSwipe({ onSwipeLeft }));

      // End touch without start - should not throw
      expect(() => {
        act(() => {
          result.current.onTouchEnd(createTouchEvent(100, 100));
        });
      }).not.toThrow();

      expect(onSwipeLeft).not.toHaveBeenCalled();
    });

    it('handles undefined handlers gracefully', () => {
      const { result } = renderHook(() => useSwipe({}));

      // Should not throw when handlers are undefined
      expect(() => {
        act(() => {
          result.current.onTouchStart(createTouchEvent(200, 100));
        });
        act(() => {
          result.current.onTouchEnd(createTouchEvent(100, 100));
        });
      }).not.toThrow();
    });

    it('uses default threshold of 50px', () => {
      const onSwipeRight = vi.fn();

      const { result } = renderHook(() => useSwipe({ onSwipeRight }));

      // Movement of 49px should not trigger
      act(() => {
        result.current.onTouchStart(createTouchEvent(100, 100));
      });
      act(() => {
        result.current.onTouchEnd(createTouchEvent(149, 100));
      });
      expect(onSwipeRight).not.toHaveBeenCalled();

      // Movement of 51px should trigger
      act(() => {
        result.current.onTouchStart(createTouchEvent(100, 100));
      });
      act(() => {
        result.current.onTouchEnd(createTouchEvent(151, 100));
      });
      expect(onSwipeRight).toHaveBeenCalledTimes(1);
    });

    it('uses custom threshold', () => {
      const onSwipeRight = vi.fn();

      const { result } = renderHook(() =>
        useSwipe({ onSwipeRight }, { threshold: 100 })
      );

      // Movement of 99px should not trigger with 100px threshold
      act(() => {
        result.current.onTouchStart(createTouchEvent(100, 100));
      });
      act(() => {
        result.current.onTouchEnd(createTouchEvent(199, 100));
      });
      expect(onSwipeRight).not.toHaveBeenCalled();

      // Movement of 101px should trigger
      act(() => {
        result.current.onTouchStart(createTouchEvent(100, 100));
      });
      act(() => {
        result.current.onTouchEnd(createTouchEvent(201, 100));
      });
      expect(onSwipeRight).toHaveBeenCalledTimes(1);
    });
  });
});
