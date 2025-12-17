import { useState, useCallback, useRef, useEffect } from 'react';
import { FEEDBACK_TIMEOUT_MS } from '@/constants/ui';

export function useFeedback(duration = FEEDBACK_TIMEOUT_MS) {
  const [success, setSuccess] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    return () => {
      if (timerRef.current) clearTimeout(timerRef.current);
    };
  }, []);

  const showSuccess = useCallback(() => {
    if (timerRef.current) clearTimeout(timerRef.current);
    setSuccess(true);
    setError(null);
    timerRef.current = setTimeout(() => setSuccess(false), duration);
  }, [duration]);

  const showError = useCallback((msg: string) => {
    setError(msg);
    setSuccess(false);
  }, []);

  const clearError = useCallback(() => setError(null), []);

  return { success, error, showSuccess, showError, clearError };
}
