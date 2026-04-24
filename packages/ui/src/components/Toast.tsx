/**
 * Minimal toast primitive — no external dependencies.
 *
 * Usage:
 *   1. Mount <ToastViewport /> once at the app root.
 *   2. Call useToast().show('Message', { variant: 'success' }) anywhere below it.
 *
 * Supports:
 *   - success / error variants using design-system color tokens
 *   - Auto-dismiss (default 2500 ms)
 *   - Stacking (multiple toasts at once)
 *   - prefers-reduced-motion (no slide animation)
 *   - aria-live="polite" for screen-reader announcements
 */

import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useRef,
  useState,
  type ReactNode,
} from 'react';
import { createPortal } from 'react-dom';
import { cn } from '../lib/cn';

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export type ToastVariant = 'success' | 'error' | 'default';

export interface ToastOptions {
  variant?: ToastVariant;
  /** Auto-dismiss delay in ms. Default: 2500. */
  durationMs?: number;
}

interface ToastEntry {
  id: number;
  message: string;
  variant: ToastVariant;
  durationMs: number;
}

interface ToastContextValue {
  show: (message: string, opts?: ToastOptions) => void;
}

// ---------------------------------------------------------------------------
// Context
// ---------------------------------------------------------------------------

const ToastContext = createContext<ToastContextValue | null>(null);

let nextId = 1;

// ---------------------------------------------------------------------------
// ToastViewport
// ---------------------------------------------------------------------------

/**
 * Mount this once in the app root. It renders a portal into document.body and
 * provides the toast context to all descendants.
 */
export function ToastViewport({ children }: { children?: ReactNode }) {
  const [toasts, setToasts] = useState<ToastEntry[]>([]);

  const show = useCallback((message: string, opts?: ToastOptions) => {
    const entry: ToastEntry = {
      id: nextId++,
      message,
      variant: opts?.variant ?? 'default',
      durationMs: opts?.durationMs ?? 2500,
    };
    setToasts((prev) => [...prev, entry]);
  }, []);

  const dismiss = useCallback((id: number) => {
    setToasts((prev) => prev.filter((t) => t.id !== id));
  }, []);

  return (
    <ToastContext.Provider value={{ show }}>
      {children}
      {typeof document !== 'undefined' &&
        createPortal(
          <div
            aria-live="polite"
            aria-atomic="false"
            className="fixed bottom-4 right-4 z-[9999] flex flex-col gap-2 pointer-events-none"
          >
            {toasts.map((toast) => (
              <ToastItem key={toast.id} toast={toast} onDismiss={dismiss} />
            ))}
          </div>,
          document.body
        )}
    </ToastContext.Provider>
  );
}

// ---------------------------------------------------------------------------
// ToastItem
// ---------------------------------------------------------------------------

interface ToastItemProps {
  toast: ToastEntry;
  onDismiss: (id: number) => void;
}

function ToastItem({ toast, onDismiss }: ToastItemProps) {
  const [visible, setVisible] = useState(false);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Trigger enter animation on mount
  useEffect(() => {
    // rAF so the initial paint is done before we add the visible class
    const raf = requestAnimationFrame(() => setVisible(true));
    return () => cancelAnimationFrame(raf);
  }, []);

  // Auto-dismiss
  useEffect(() => {
    timerRef.current = setTimeout(() => {
      setVisible(false);
      // Wait for exit transition before removing from DOM
      setTimeout(() => onDismiss(toast.id), 200);
    }, toast.durationMs);
    return () => {
      if (timerRef.current) clearTimeout(timerRef.current);
    };
  }, [toast.id, toast.durationMs, onDismiss]);

  return (
    <div
      role="status"
      className={cn(
        // Base layout
        'pointer-events-auto flex items-center gap-2 px-4 py-2.5 rounded-md',
        'border shadow-md text-sm font-medium',
        // Transition — respects prefers-reduced-motion via Tailwind motion-safe:
        'transition-all duration-200 motion-reduce:transition-none',
        visible
          ? 'opacity-100 translate-y-0'
          : 'opacity-0 translate-y-2',
        // Variants
        toast.variant === 'success' && 'bg-panel border-border text-success',
        toast.variant === 'error' && 'bg-panel border-border text-error',
        toast.variant === 'default' && 'bg-panel border-border text-high',
      )}
    >
      {toast.variant === 'success' && (
        <span aria-hidden className="shrink-0 text-success">✓</span>
      )}
      {toast.variant === 'error' && (
        <span aria-hidden className="shrink-0 text-error">✕</span>
      )}
      {toast.message}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Hook
// ---------------------------------------------------------------------------

/**
 * Returns { show } for displaying toasts. Must be used inside <ToastViewport>.
 */
export function useToast(): ToastContextValue {
  const ctx = useContext(ToastContext);
  if (!ctx) {
    // Graceful fallback — log a warning but don't crash if ToastViewport is
    // missing (e.g. in tests or Storybook).
    return {
      show: (message, _opts) => {
        console.warn('[Toast] ToastViewport is not mounted. Message:', message);
      },
    };
  }
  return ctx;
}
