import { useSyncExternalStore } from 'react';

/**
 * Threshold below which the conversation nav overlay's vertical 4-button
 * stack would overlap the chat input box on narrow chat shells. The value
 * matches the design intent in `docs/superpowers/plans/2026-05-04-chat-nav-overlay-fixes.md`
 * and is intentionally narrower than the global `useIsMobile` breakpoint
 * (767px), which targets full app shell layouts rather than the chat column.
 */
const NARROW_BREAKPOINT_PX = 480;
const query = `(max-width: ${NARROW_BREAKPOINT_PX}px)`;

let mediaQuery: MediaQueryList | null = null;

function getMediaQuery() {
  if (!mediaQuery) {
    mediaQuery = window.matchMedia(query);
  }
  return mediaQuery;
}

function subscribe(callback: () => void) {
  const mq = getMediaQuery();
  mq.addEventListener('change', callback);
  return () => mq.removeEventListener('change', callback);
}

function getSnapshot() {
  return getMediaQuery().matches;
}

function getServerSnapshot() {
  // SSR: assume desktop. The first client paint will resolve correctly.
  return false;
}

/**
 * Returns true when the viewport is at or below the chat-nav narrow
 * breakpoint (480px). Used by chat shells to suppress the floating nav
 * overlay so it doesn't overlap the chat input.
 *
 * Modelled on `useIsMobile`, which uses a coarser 767px breakpoint for
 * whole-app responsive layouts.
 */
export function useNarrowViewport(): boolean {
  return useSyncExternalStore(subscribe, getSnapshot, getServerSnapshot);
}
