import { useEffect, useState } from 'react';

/**
 * Returns `true` while the document is in the foreground.
 *
 * When the document becomes hidden, we wait `graceMs` milliseconds before
 * reporting `false`, so that trivial tab switches (Cmd-Tab, peeking at
 * another tab) don't tear down active subscriptions. The moment the document
 * becomes visible again the hook returns `true` regardless of timer state.
 */
export function useDocumentVisible(graceMs = 30_000): boolean {
  const [visible, setVisible] = useState<boolean>(() => {
    if (typeof document === 'undefined') return true;
    return !document.hidden;
  });

  useEffect(() => {
    if (typeof document === 'undefined') return;

    let timer: ReturnType<typeof setTimeout> | null = null;

    const handleVisibilityChange = () => {
      if (document.hidden) {
        if (timer !== null) return;
        timer = setTimeout(() => {
          timer = null;
          setVisible(false);
        }, graceMs);
      } else {
        if (timer !== null) {
          clearTimeout(timer);
          timer = null;
        }
        setVisible(true);
      }
    };

    document.addEventListener('visibilitychange', handleVisibilityChange);
    return () => {
      document.removeEventListener('visibilitychange', handleVisibilityChange);
      if (timer !== null) clearTimeout(timer);
    };
  }, [graceMs]);

  return visible;
}
