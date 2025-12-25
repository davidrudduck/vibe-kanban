import { useMediaQuery } from './useMediaQuery';

/**
 * Hook to detect if the current viewport is mobile-sized.
 *
 * Uses Tailwind's `sm` breakpoint (640px) as the default threshold.
 * Returns `true` when viewport width is below the breakpoint.
 *
 * @param breakpoint - Maximum width in pixels to consider as mobile (default: 640)
 * @returns boolean indicating if viewport is mobile-sized
 *
 * @example
 * ```tsx
 * function MyComponent() {
 *   const isMobile = useIsMobile();
 *
 *   if (isMobile) {
 *     return <MobileView />;
 *   }
 *   return <DesktopView />;
 * }
 * ```
 */
export function useIsMobile(breakpoint = 640): boolean {
  // max-width query returns true when viewport is smaller than breakpoint
  return useMediaQuery(`(max-width: ${breakpoint - 1}px)`);
}
