import { useRef, useCallback, TouchEvent } from 'react';

interface SwipeHandlers {
  onSwipeLeft?: () => void;
  onSwipeRight?: () => void;
}

interface SwipeConfig {
  /** Minimum distance (in pixels) to be considered a swipe */
  threshold?: number;
  /** Maximum time (in ms) for the swipe gesture */
  maxTime?: number;
}

interface UseSwipeReturn {
  onTouchStart: (e: TouchEvent) => void;
  onTouchEnd: (e: TouchEvent) => void;
}

/**
 * Hook for detecting horizontal swipe gestures on touch devices.
 * Returns touch event handlers to attach to the swipeable element.
 */
export function useSwipe(
  handlers: SwipeHandlers,
  config: SwipeConfig = {}
): UseSwipeReturn {
  const { threshold = 50, maxTime = 300 } = config;

  const touchStartRef = useRef<{
    x: number;
    y: number;
    time: number;
  } | null>(null);

  const onTouchStart = useCallback((e: TouchEvent) => {
    const touch = e.touches[0];
    if (touch) {
      touchStartRef.current = {
        x: touch.clientX,
        y: touch.clientY,
        time: Date.now(),
      };
    }
  }, []);

  const onTouchEnd = useCallback(
    (e: TouchEvent) => {
      const start = touchStartRef.current;
      if (!start) return;

      const touch = e.changedTouches[0];
      if (!touch) return;

      const deltaX = touch.clientX - start.x;
      const deltaY = touch.clientY - start.y;
      const deltaTime = Date.now() - start.time;

      touchStartRef.current = null;

      // Check if it's a valid horizontal swipe:
      // - Horizontal distance exceeds threshold
      // - Horizontal movement is greater than vertical (to avoid scrolling)
      // - Time is within max time
      if (
        Math.abs(deltaX) > threshold &&
        Math.abs(deltaX) > Math.abs(deltaY) &&
        deltaTime <= maxTime
      ) {
        if (deltaX > 0) {
          handlers.onSwipeRight?.();
        } else {
          handlers.onSwipeLeft?.();
        }
      }
    },
    [handlers, threshold, maxTime]
  );

  return { onTouchStart, onTouchEnd };
}
