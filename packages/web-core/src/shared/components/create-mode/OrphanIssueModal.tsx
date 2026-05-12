import { useEffect, useRef } from 'react';
import { SpinnerIcon, WarningIcon } from '@phosphor-icons/react';
import { cn } from '@/shared/lib/utils';

export interface OrphanIssueState {
  id: string;
  title: string;
  simpleId: string;
  remoteProjectId: string;
}

interface OrphanIssueModalProps {
  orphan: OrphanIssueState;
  isRetrying: boolean;
  onRetry: () => void;
  onRemove: () => void;
}

/**
 * Blocking overlay shown when workspace creation fails after a kanban issue
 * was already created. Forces the user to either retry the workspace or remove
 * the orphaned issue — no silent dismiss.
 */
export function OrphanIssueModal({
  orphan,
  isRetrying,
  onRetry,
  onRemove,
}: OrphanIssueModalProps) {
  const isBusy = isRetrying;
  const dialogRef = useRef<HTMLDivElement>(null);

  // Focus trap: prevent Tab from leaving the modal (WCAG 2.1.2)
  useEffect(() => {
    const dialog = dialogRef.current;
    if (!dialog) return;

    const focusableSelector =
      'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"]):not([disabled])';

    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key !== 'Tab') return;

      const focusableElements = Array.from(
        dialog.querySelectorAll<HTMLElement>(focusableSelector)
      );
      if (focusableElements.length === 0) return;

      const firstElement = focusableElements[0];
      const lastElement = focusableElements[focusableElements.length - 1];

      if (e.shiftKey && document.activeElement === firstElement) {
        e.preventDefault();
        lastElement?.focus();
      } else if (!e.shiftKey && document.activeElement === lastElement) {
        e.preventDefault();
        firstElement?.focus();
      }
    };

    dialog.addEventListener('keydown', handleKeyDown);
    return () => dialog.removeEventListener('keydown', handleKeyDown);
  }, []);

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-primary/80 backdrop-blur-sm"
      onKeyDown={(e) => {
        // Prevent any parent Escape handlers from dismissing this modal.
        // The user must choose Retry or Remove — there is no silent dismiss.
        if (e.key === 'Escape') {
          e.preventDefault();
          e.stopPropagation();
          e.nativeEvent.stopImmediatePropagation();
        }
      }}
    >
      <div
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby="orphan-modal-title"
        className="mx-base flex w-full max-w-md flex-col gap-base rounded-sm border border-border/60 bg-panel p-base shadow-lg"
      >
        {/* Header */}
        <div className="flex items-start gap-half">
          <WarningIcon
            className="mt-px size-icon-sm shrink-0 text-error"
            weight="fill"
          />
          <div className="flex flex-col gap-quarter">
            <h2
              id="orphan-modal-title"
              className="text-sm font-semibold text-high"
            >
              Workspace failed to start
            </h2>
            <p className="text-sm text-normal">
              Task{' '}
              <span className="font-mono text-xs text-brand">
                {orphan.simpleId}
              </span>{' '}
              <span className="font-medium">&ldquo;{orphan.title}&rdquo;</span>{' '}
              was added to the board, but the workspace could not be started.
            </p>
            <p className="text-xs text-low">
              Choose an action — this dialog cannot be dismissed without
              resolving the orphaned task.
            </p>
          </div>
        </div>

        {/* Actions */}
        <div className="flex flex-col gap-half">
          <button
            // eslint-disable-next-line jsx-a11y/no-autofocus
            autoFocus
            type="button"
            onClick={onRetry}
            disabled={isBusy}
            className={cn(
              'flex items-center justify-center gap-half rounded-sm px-base py-half',
              'bg-brand text-sm font-medium text-white',
              'hover:bg-brand/90 disabled:cursor-not-allowed disabled:opacity-50',
              'transition-colors'
            )}
          >
            {isRetrying ? (
              <SpinnerIcon className="size-icon-xs animate-spin" />
            ) : null}
            <span>Retry workspace creation</span>
          </button>

          <button
            type="button"
            onClick={onRemove}
            disabled={isBusy}
            className={cn(
              'flex items-center justify-center gap-half rounded-sm px-base py-half',
              'border border-error/40 text-sm font-medium text-error',
              'hover:bg-error/10 disabled:cursor-not-allowed disabled:opacity-50',
              'transition-colors'
            )}
          >
            <span>Remove task from board</span>
          </button>
        </div>
      </div>
    </div>
  );
}
