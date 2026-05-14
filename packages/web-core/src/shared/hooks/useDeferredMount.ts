import { useEffect, useState } from 'react';

/**
 * Returns `false` on the first render and `true` after the browser has been
 * idle once (or after a fallback timeout in environments without
 * requestIdleCallback). Use to defer non-critical subscriptions so they do
 * not block first paint.
 */
export function useDeferredMount(fallbackMs = 200): boolean {
  const [mounted, setMounted] = useState(false);

  useEffect(() => {
    let cancelled = false;
    const idle = globalThis as unknown as {
      requestIdleCallback?: (cb: () => void) => number;
      cancelIdleCallback?: (id: number) => void;
    };

    if (typeof idle.requestIdleCallback === 'function') {
      const id = idle.requestIdleCallback(() => {
        if (!cancelled) setMounted(true);
      });
      return () => {
        cancelled = true;
        idle.cancelIdleCallback?.(id);
      };
    }

    const timer = setTimeout(() => {
      if (!cancelled) setMounted(true);
    }, fallbackMs);
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [fallbackMs]);

  return mounted;
}
