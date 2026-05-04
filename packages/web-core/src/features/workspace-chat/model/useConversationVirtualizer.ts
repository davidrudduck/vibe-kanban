/**
 * Conversation Virtualizer Hook
 *
 * Shared TanStack Virtual configuration for the conversation list.
 * Owns the virtualizer instance, measurement, and imperative scroll helpers.
 */

import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type RefObject,
} from 'react';
import {
  useVirtualizer,
  measureElement as defaultMeasureElement,
} from '@tanstack/react-virtual';
import type { Virtualizer, VirtualItem } from '@tanstack/react-virtual';

import {
  type ConversationRow,
  SIZE_ESTIMATE_PX,
  estimateSizeForRow,
  findNextUserMessageIndex,
  findPreviousUserMessageIndex,
} from './conversation-row-model';
import {
  NEAR_BOTTOM_THRESHOLD_PX,
  isNearBottom,
} from './conversation-scroll-commands';

// TanStack Virtual's ScrollBehavior ('auto' | 'smooth' | 'instant') shadows
// the DOM ScrollBehavior. Use a narrow type to avoid TS2322 mismatches.
type ScrollToOptionsBehavior = 'auto' | 'smooth';

// ---------------------------------------------------------------------------
// Module-level helpers
// ---------------------------------------------------------------------------

/** Returns the index of the last user message in rows, or -1 if none. */
function findLastUserMessageIndex(rows: readonly ConversationRow[]): number {
  for (let i = rows.length - 1; i >= 0; i--) {
    if (rows[i].isUserMessage) return i;
  }
  return -1;
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/** Number of items to render beyond the visible area in each direction. */
const OVERSCAN = 8;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface ConversationVirtualizerOptions {
  /** The semantic row model driving the list (virtualized head only). */
  rows: ConversationRow[];

  /**
   * Total number of conversation rows (virtualized + unvirtualized tail).
   * The bottom-lock correction must fire when ANY row is added — including
   * unvirtualized tail rows that don't change `rows.length` or `totalSize`.
   * Without this, streaming entries appended to the tail silently grow the
   * scroll container while the correction never fires.
   */
  totalRowCount: number;

  /** Ref to the scrollable container element. */
  scrollContainerRef: RefObject<HTMLDivElement | null>;

  /**
   * Called when the at-bottom state changes. Shells use this to show/hide
   * the scroll-to-bottom affordance.
   */
  onAtBottomChange?: (atBottom: boolean) => void;

  /**
   * Called when the at-top state changes. Shells use this to show/hide
   * the scroll-to-top affordance.
   */
  onAtTopChange?: (atTop: boolean) => void;

  shouldSuppressSizeAdjustment?: () => boolean;
}

export interface ConversationVirtualizerResult {
  /** The TanStack Virtual virtualizer instance. */
  virtualizer: Virtualizer<HTMLDivElement, Element>;

  /** Virtual items currently in the render window (including overscan). */
  virtualItems: VirtualItem[];

  /** Total pixel size of all items (for the scroll spacer). */
  totalSize: number;

  /**
   * Ref callback for row DOM elements. Attach to each rendered row's
   * container element alongside `data-index={virtualItem.index}`.
   * TanStack Virtual uses this to measure real DOM heights and attach
   * a ResizeObserver for automatic re-measurement on size changes.
   */
  measureElement: (node: Element | null) => void;

  /** Scroll to the absolute bottom of the list. */
  scrollToBottom: (behavior?: ScrollToOptionsBehavior) => void;

  /** Scroll to the absolute top of the list. */
  scrollToTop: (behavior?: ScrollToOptionsBehavior) => void;

  /** Scroll to a specific row index. */
  scrollToIndex: (
    index: number,
    options?: {
      align?: 'start' | 'center' | 'end';
      behavior?: ScrollToOptionsBehavior;
    }
  ) => void;

  /**
   * Scroll to the previous user message relative to the first visible item.
   * Returns true if a target was found and scrolled to, false otherwise.
   */
  scrollToPreviousUserMessage: () => boolean;

  /**
   * Scroll to the next user message after the first visible item.
   * Returns true if a target was found and scrolled to, false otherwise.
   */
  scrollToNextUserMessage: () => boolean;

  /**
   * Whether at least one user message exists before the current scroll position
   * (or before the most recent user message when the scroll container is unmounted).
   */
  hasPreviousUserMessage: () => boolean;

  /**
   * Whether at least one user message exists after the current scroll position
   * (or after the most recent user message when the scroll container is unmounted).
   */
  hasNextUserMessage: () => boolean;

  /**
   * Whether the scroll container is currently near the bottom.
   * Reactive — updates via scroll event listener, not just point-in-time.
   */
  isAtBottom: boolean;

  /**
   * Whether the scroll container is currently at the top.
   * Reactive — updates via scroll event listener, not just point-in-time.
   */
  isAtTop: boolean;

  /** Point-in-time check (non-reactive). Reads DOM directly. */
  checkIsAtBottom: () => boolean;

  /**
   * Release the bottom-lock. Call when navigating away from the
   * bottom (e.g., scrollToPreviousUserMessage).
   */
  releaseBottomLock: () => void;

  /**
   * Look up the ConversationRow index for a given virtual item.
   * Since our virtualizer uses identity mapping (no lane reordering),
   * this is simply `virtualItem.index`.
   */
  rowIndexForVirtualItem: (item: VirtualItem) => number;

  /**
   * Look up the ConversationRow for a given virtual item.
   * Returns undefined if the index is out of bounds.
   */
  rowForVirtualItem: (item: VirtualItem) => ConversationRow | undefined;

  /**
   * Returns whether bottom-lock auto-follow is currently armed. Used internally
   * by `syncScrollEdges` and exposed on the result so unit tests can assert
   * lock state without reaching into refs.
   *
   * @internal
   */
  isBottomScrollCorrectionActive: () => boolean;
}

// ---------------------------------------------------------------------------
// Hook
// ---------------------------------------------------------------------------

/**
 * Configure and return a TanStack Virtual virtualizer for the conversation list.
 *
 * This hook is the single source of virtualizer configuration. It is consumed
 * by `ConversationListContainer` and must not be duplicated across shells.
 */
export function useConversationVirtualizer({
  rows,
  totalRowCount,
  scrollContainerRef,
  onAtBottomChange,
  onAtTopChange,
  shouldSuppressSizeAdjustment,
}: ConversationVirtualizerOptions): ConversationVirtualizerResult {
  const bottomLockedRef = useRef(false);
  const smoothScrollDeadlineRef = useRef(0);

  const isBottomScrollCorrectionActive = useCallback(
    () => bottomLockedRef.current,
    []
  );

  // -------------------------------------------------------------------------
  // Virtualizer instance
  // -------------------------------------------------------------------------

  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scrollContainerRef.current,
    estimateSize: (index) => {
      const row = rows[index];
      if (!row) return SIZE_ESTIMATE_PX.medium;
      const containerWidth = scrollContainerRef.current?.clientWidth ?? null;
      return estimateSizeForRow(row, containerWidth);
    },
    getItemKey: (index) => {
      const row = rows[index];
      return row ? row.semanticKey : index;
    },
    overscan: OVERSCAN,
    measureElement: defaultMeasureElement,
    useAnimationFrameWithResizeObserver: false,
  });

  // -------------------------------------------------------------------------
  // shouldAdjustScrollPositionOnItemSizeChange
  //
  // Preserve the reader's position only when a row fully above the viewport
  // changes size. Mid-list flicker happens when we compensate for rows that
  // are still visible or below the viewport, because those corrections can
  // move the render window and trigger another measurement pass.
  // -------------------------------------------------------------------------

  useEffect(() => {
    virtualizer.shouldAdjustScrollPositionOnItemSizeChange = (
      item,
      _delta,
      instance
    ) => {
      const scrollElement = scrollContainerRef.current;
      const viewportHeight =
        scrollElement?.clientHeight ?? instance.scrollRect?.height ?? 0;
      const scrollOffset =
        scrollElement?.scrollTop ?? instance.scrollOffset ?? 0;
      const totalScrollableSize =
        scrollElement?.scrollHeight ?? instance.getTotalSize();
      const remainingDistance =
        totalScrollableSize - (scrollOffset + viewportHeight);
      const isItemFullyAboveViewport = item.end <= scrollOffset;
      const isBottomLocked = bottomLockedRef.current;

      const shouldAdjust =
        !isBottomLocked &&
        !shouldSuppressSizeAdjustment?.() &&
        isItemFullyAboveViewport &&
        remainingDistance > NEAR_BOTTOM_THRESHOLD_PX;

      return shouldAdjust;
    };

    return () => {
      virtualizer.shouldAdjustScrollPositionOnItemSizeChange = undefined;
    };
  }, [shouldSuppressSizeAdjustment, virtualizer]);

  // -------------------------------------------------------------------------
  // Reactive isAtBottom state
  // -------------------------------------------------------------------------

  const [isAtBottomState, setIsAtBottomState] = useState(true);
  const [isAtTopState, setIsAtTopState] = useState(true);
  const onAtBottomChangeRef = useRef(onAtBottomChange);
  onAtBottomChangeRef.current = onAtBottomChange;
  const onAtTopChangeRef = useRef(onAtTopChange);
  onAtTopChangeRef.current = onAtTopChange;
  const lastAtBottomRef = useRef(true);
  const lastAtTopRef = useRef(true);

  const syncScrollEdges = useCallback(() => {
    const el = scrollContainerRef.current;
    const nextAtBottom = isBottomScrollCorrectionActive()
      ? true
      : el
        ? isNearBottom(el.scrollTop, el.clientHeight, el.scrollHeight)
        : true;
    const nextAtTop = el ? el.scrollTop <= 0 : true;

    // Re-arm bottom-lock when the user has scrolled back to the bottom edge
    // after a manual release. Skipped while a programmatic scroll is in flight
    // (see `smoothScrollDeadlineRef`) to avoid re-arming on the first tick of
    // a smooth scrollToTop / scrollToIndex animation, which would interrupt
    // the animation via the bottom-snap layout effect.
    if (
      nextAtBottom &&
      !bottomLockedRef.current &&
      performance.now() > smoothScrollDeadlineRef.current
    ) {
      bottomLockedRef.current = true;
    }

    if (nextAtBottom !== lastAtBottomRef.current) {
      lastAtBottomRef.current = nextAtBottom;
      setIsAtBottomState(nextAtBottom);
      onAtBottomChangeRef.current?.(nextAtBottom);
    } else {
      setIsAtBottomState((current) =>
        current === nextAtBottom ? current : nextAtBottom
      );
    }

    if (nextAtTop !== lastAtTopRef.current) {
      lastAtTopRef.current = nextAtTop;
      setIsAtTopState(nextAtTop);
      onAtTopChangeRef.current?.(nextAtTop);
    } else {
      setIsAtTopState((current) =>
        current === nextAtTop ? current : nextAtTop
      );
    }
  }, [isBottomScrollCorrectionActive, scrollContainerRef]);

  const prevScrollTopRef = useRef(0);

  useEffect(() => {
    const el = scrollContainerRef.current;
    if (!el) return;

    prevScrollTopRef.current = el.scrollTop;

    const handleScroll = () => {
      const currentScrollTop = el.scrollTop;

      // Release bottom lock on any user-initiated upward scroll.
      // Guards prevent false positives from programmatic scroll sources:
      // - smoothScrollDeadlineRef: set during scrollToBottom('smooth')
      // - shouldSuppressSizeAdjustment: set during interaction anchor corrections
      // - 5px threshold: filters input-resize micro-adjustments
      if (
        bottomLockedRef.current &&
        prevScrollTopRef.current - currentScrollTop > 5 &&
        performance.now() > smoothScrollDeadlineRef.current &&
        !shouldSuppressSizeAdjustment?.()
      ) {
        bottomLockedRef.current = false;
      }

      prevScrollTopRef.current = currentScrollTop;
      syncScrollEdges();
    };

    el.addEventListener('scroll', handleScroll, { passive: true });
    handleScroll();

    return () => {
      el.removeEventListener('scroll', handleScroll);
    };
  }, [scrollContainerRef, shouldSuppressSizeAdjustment, syncScrollEdges]);

  // -------------------------------------------------------------------------
  // Derived state
  // -------------------------------------------------------------------------

  const virtualItems = virtualizer.getVirtualItems();
  const totalSize = virtualizer.getTotalSize();

  useLayoutEffect(() => {
    syncScrollEdges();

    if (!bottomLockedRef.current) return;
    if (performance.now() < smoothScrollDeadlineRef.current) return;

    const el = scrollContainerRef.current;
    if (!el) return;

    const maxScroll = el.scrollHeight - el.clientHeight;
    if (maxScroll > 0 && Math.abs(maxScroll - el.scrollTop) > 1) {
      el.scrollTop = maxScroll;
    }
  }, [
    rows.length,
    totalRowCount,
    totalSize,
    syncScrollEdges,
    scrollContainerRef,
  ]);

  // -------------------------------------------------------------------------
  // Imperative helpers
  // -------------------------------------------------------------------------

  const scrollToBottom = useCallback(
    (behavior: ScrollToOptionsBehavior = 'smooth') => {
      const el = scrollContainerRef.current;
      if (!el) return;

      bottomLockedRef.current = true;

      if (behavior === 'smooth') {
        smoothScrollDeadlineRef.current = performance.now() + 500;
        el.scrollTo({ top: el.scrollHeight, behavior: 'smooth' });
      } else {
        el.scrollTop = el.scrollHeight - el.clientHeight;
      }
    },
    [scrollContainerRef, virtualizer]
  );

  const scrollToTop = useCallback(
    (behavior: ScrollToOptionsBehavior = 'smooth') => {
      const el = scrollContainerRef.current;
      if (!el) return;

      // Releasing the bottom lock is required so the layout-effect doesn't
      // immediately snap back to the bottom on the next render.
      bottomLockedRef.current = false;

      if (behavior === 'smooth') {
        smoothScrollDeadlineRef.current = performance.now() + 500;
        el.scrollTo({ top: 0, behavior: 'smooth' });
      } else {
        el.scrollTop = 0;
      }
    },
    [scrollContainerRef]
  );

  const scrollToIndex = useCallback(
    (
      index: number,
      options?: {
        align?: 'start' | 'center' | 'end';
        behavior?: ScrollToOptionsBehavior;
      }
    ) => {
      if (bottomLockedRef.current) {
        bottomLockedRef.current = false;
      }

      const resolvedBehavior = options?.behavior ?? 'smooth';
      if (resolvedBehavior === 'smooth') {
        smoothScrollDeadlineRef.current = performance.now() + 500;
      }

      virtualizer.scrollToIndex(index, {
        align: options?.align ?? 'start',
        behavior: resolvedBehavior,
      });
    },
    [virtualizer]
  );

  const hasPreviousUserMessage = useCallback((): boolean => {
    if (rows.length === 0) return false;
    const scrollEl = scrollContainerRef.current;
    const scrollTop = scrollEl?.scrollTop ?? 0;
    const fromIndex =
      (scrollEl && scrollTop > 0
        ? virtualizer.getVirtualItemForOffset(scrollTop)?.index
        : undefined) ?? findLastUserMessageIndex(rows);
    if (fromIndex < 0) return false;
    return findPreviousUserMessageIndex(rows, fromIndex) !== -1;
  }, [scrollContainerRef, virtualizer, rows]);

  const hasNextUserMessage = useCallback((): boolean => {
    if (rows.length === 0) return false;
    const scrollEl = scrollContainerRef.current;
    const scrollTop = scrollEl?.scrollTop ?? 0;
    const fromIndex =
      (scrollEl && scrollTop > 0
        ? virtualizer.getVirtualItemForOffset(scrollTop)?.index
        : undefined) ?? findLastUserMessageIndex(rows);
    if (fromIndex < 0) return false;
    return findNextUserMessageIndex(rows, fromIndex) !== -1;
  }, [scrollContainerRef, virtualizer, rows]);

  const scrollToPreviousUserMessage = useCallback((): boolean => {
    const scrollEl = scrollContainerRef.current;
    const items = virtualizer.getVirtualItems();
    if (items.length === 0 || rows.length === 0 || !scrollEl) return false;

    const firstVisibleIndex =
      virtualizer.getVirtualItemForOffset(scrollEl.scrollTop)?.index ??
      items[0].index;
    const targetIndex = findPreviousUserMessageIndex(rows, firstVisibleIndex);

    if (targetIndex < 0) return false;

    smoothScrollDeadlineRef.current = performance.now() + 500;
    virtualizer.scrollToIndex(targetIndex, {
      align: 'start',
      behavior: 'smooth',
    });
    return true;
  }, [scrollContainerRef, virtualizer, rows]);

  const scrollToNextUserMessage = useCallback((): boolean => {
    const scrollEl = scrollContainerRef.current;
    const items = virtualizer.getVirtualItems();
    if (items.length === 0 || rows.length === 0 || !scrollEl) return false;

    const firstVisibleIndex =
      virtualizer.getVirtualItemForOffset(scrollEl.scrollTop)?.index ??
      items[0].index;
    const targetIndex = findNextUserMessageIndex(rows, firstVisibleIndex);

    if (targetIndex < 0) return false;

    smoothScrollDeadlineRef.current = performance.now() + 500;
    virtualizer.scrollToIndex(targetIndex, {
      align: 'start',
      behavior: 'smooth',
    });
    return true;
  }, [scrollContainerRef, virtualizer, rows]);

  const checkIsAtBottom = useCallback((): boolean => {
    const el = scrollContainerRef.current;
    if (!el) return true;
    return isNearBottom(el.scrollTop, el.clientHeight, el.scrollHeight);
  }, [scrollContainerRef]);

  const releaseBottomLock = useCallback(() => {
    if (!bottomLockedRef.current) return;
    bottomLockedRef.current = false;
  }, []);

  // -------------------------------------------------------------------------
  // Row ↔ VirtualItem mapping
  // -------------------------------------------------------------------------

  const rowIndexForVirtualItem = useCallback(
    (item: VirtualItem): number => item.index,
    []
  );

  const rowForVirtualItem = useCallback(
    (item: VirtualItem): ConversationRow | undefined => rows[item.index],
    [rows]
  );

  const measureElement = useCallback(
    (node: Element | null) => {
      virtualizer.measureElement(node);
    },
    [virtualizer]
  );

  // -------------------------------------------------------------------------
  // Return
  // -------------------------------------------------------------------------

  return {
    virtualizer,
    virtualItems,
    totalSize,
    measureElement,
    scrollToBottom,
    scrollToTop,
    scrollToIndex,
    scrollToPreviousUserMessage,
    scrollToNextUserMessage,
    hasPreviousUserMessage,
    hasNextUserMessage,
    isAtBottom: isAtBottomState,
    isAtTop: isAtTopState,
    checkIsAtBottom,
    releaseBottomLock,
    rowIndexForVirtualItem,
    rowForVirtualItem,
    isBottomScrollCorrectionActive,
  };
}
