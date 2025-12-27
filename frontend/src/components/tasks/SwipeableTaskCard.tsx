import {
  useRef,
  useState,
  useCallback,
  type ReactNode,
  type TouchEvent,
} from 'react';
import { Archive } from 'lucide-react';
import { useIsMobile } from '@/hooks/useIsMobile';
import { cn } from '@/lib/utils';
import type { TaskWithAttemptStatus } from 'shared/types';

interface SwipeableTaskCardProps {
  task: TaskWithAttemptStatus;
  onArchive: (task: TaskWithAttemptStatus) => void;
  isArchived: boolean;
  disabled?: boolean;
  children: ReactNode;
}

const SWIPE_THRESHOLD = 100; // pixels needed to trigger archive
const MAX_SWIPE = 150; // maximum swipe distance for visual capping

/**
 * Wrapper component that adds swipe-to-archive functionality for mobile devices.
 * Wraps around TaskCard content and handles touch gestures.
 */
export function SwipeableTaskCard({
  task,
  onArchive,
  isArchived,
  disabled = false,
  children,
}: SwipeableTaskCardProps) {
  const isMobile = useIsMobile();
  const touchStartRef = useRef<{ x: number; y: number } | null>(null);
  const currentOffsetRef = useRef(0);
  const containerRef = useRef<HTMLDivElement>(null);
  const slidingRef = useRef<HTMLDivElement>(null);

  // Track swipe state for UI updates
  const [swipeOffset, setSwipeOffset] = useState(0);

  // Disable swipe for archived tasks, remote tasks, or when explicitly disabled
  const isSwipeDisabled = disabled || isArchived;

  const handleTouchStart = useCallback(
    (e: TouchEvent) => {
      if (isSwipeDisabled) return;

      const touch = e.touches[0];
      if (touch) {
        touchStartRef.current = {
          x: touch.clientX,
          y: touch.clientY,
        };
        currentOffsetRef.current = 0;
        setSwipeOffset(0);

        // Remove transition during active swipe
        if (slidingRef.current) {
          slidingRef.current.style.transition = 'none';
        }
      }
    },
    [isSwipeDisabled]
  );

  const handleTouchMove = useCallback(
    (e: TouchEvent) => {
      if (isSwipeDisabled || !touchStartRef.current) return;

      const touch = e.touches[0];
      if (!touch) return;

      const deltaX = touch.clientX - touchStartRef.current.x;
      const deltaY = touch.clientY - touchStartRef.current.y;

      // Only handle horizontal swipes (swipe left for archive)
      // Ignore if vertical movement is greater (scrolling)
      if (Math.abs(deltaY) > Math.abs(deltaX)) {
        return;
      }

      // Only allow left swipe (negative deltaX) for archive
      if (deltaX >= 0) {
        // Reset if swiping right
        currentOffsetRef.current = 0;
        setSwipeOffset(0);
        if (slidingRef.current) {
          slidingRef.current.style.transform = 'translateX(0px)';
        }
        return;
      }

      // Prevent scrolling during horizontal swipe
      e.preventDefault();

      // Cap the swipe distance
      const offset = Math.max(deltaX, -MAX_SWIPE);
      currentOffsetRef.current = offset;
      setSwipeOffset(offset);

      // Update sliding content position
      if (slidingRef.current) {
        slidingRef.current.style.transform = `translateX(${offset}px)`;
      }
    },
    [isSwipeDisabled]
  );

  const handleTouchEnd = useCallback(() => {
    if (isSwipeDisabled || !touchStartRef.current) return;

    const offset = currentOffsetRef.current;

    // Add transition for snap animation
    if (slidingRef.current) {
      slidingRef.current.style.transition = 'transform 200ms ease-out';
    }

    // Check if swipe exceeded threshold
    if (Math.abs(offset) >= SWIPE_THRESHOLD) {
      // Archive the task
      onArchive(task);
    }

    // Reset position
    if (slidingRef.current) {
      slidingRef.current.style.transform = 'translateX(0px)';
    }

    touchStartRef.current = null;
    currentOffsetRef.current = 0;
    setSwipeOffset(0);
  }, [isSwipeDisabled, onArchive, task]);

  // On desktop, just render children without swipe wrapper
  if (!isMobile) {
    return <>{children}</>;
  }

  // Calculate if we should show the archive indicator
  const showIndicator = Math.abs(swipeOffset) > 0;
  const isOverThreshold = Math.abs(swipeOffset) >= SWIPE_THRESHOLD;

  return (
    <div
      ref={containerRef}
      data-testid="swipeable-task-card"
      className="relative overflow-hidden touch-pan-y"
      onTouchStart={handleTouchStart}
      onTouchMove={handleTouchMove}
      onTouchEnd={handleTouchEnd}
    >
      {/* Archive indicator background (revealed when swiping left) */}
      {!isSwipeDisabled && (
        <div
          data-testid="archive-indicator"
          className={cn(
            'absolute inset-y-0 right-0 flex items-center justify-end px-4',
            'bg-destructive text-destructive-foreground',
            'transition-opacity duration-150',
            showIndicator ? 'opacity-100' : 'opacity-0'
          )}
          style={{ width: MAX_SWIPE }}
        >
          <Archive
            data-testid="archive-icon"
            className={cn(
              'h-6 w-6 transition-transform duration-150',
              isOverThreshold ? 'scale-125' : 'scale-100'
            )}
          />
        </div>
      )}

      {/* Sliding content wrapper */}
      <div
        ref={slidingRef}
        data-testid="sliding-content"
        className="relative bg-background"
        style={{ transform: 'translateX(0px)' }}
      >
        {children}
      </div>
    </div>
  );
}
