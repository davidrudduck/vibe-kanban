import { useState, useCallback, useRef, useEffect } from 'react';
import { FEEDBACK_TIMEOUT_MS } from '@/constants/ui';

/**
 * Hook for managing UI feedback state with automatic cleanup.
 *
 * @param duration - Timeout duration in ms (default: FEEDBACK_TIMEOUT_MS)
 * @returns success (boolean or string message), error, and control functions
 */
export function useFeedback(duration = FEEDBACK_TIMEOUT_MS) {
  const [success, setSuccess] = useState<boolean | string>(false);
  const [error, setError] = useState<string | null>(null);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    return () => {
      if (timerRef.current) clearTimeout(timerRef.current);
    };
  }, []);

  const showSuccess = useCallback(
    (message?: string) => {
      if (timerRef.current) clearTimeout(timerRef.current);
      setSuccess(message ?? true);
      setError(null);
      timerRef.current = setTimeout(() => setSuccess(false), duration);
    },
    [duration]
  );

  const showError = useCallback((msg: string) => {
    setError(msg);
    setSuccess(false);
  }, []);

  const clearError = useCallback(() => setError(null), []);
  const clearSuccess = useCallback(() => setSuccess(false), []);

  return { success, error, showSuccess, showError, clearError, clearSuccess };
}
