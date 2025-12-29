import * as React from 'react';
import { useTranslation } from 'react-i18next';
import { motion, AnimatePresence } from 'framer-motion';
import { RefreshCw, FolderPlus } from 'lucide-react';

import { Button } from '@/components/ui/button';
import { cn } from '@/lib/utils';

export interface ParentWorktreeDialogProps {
  /**
   * Whether the dialog is open
   */
  open: boolean;

  /**
   * Callback when the dialog should close
   */
  onOpenChange: (open: boolean) => void;

  /**
   * The parent task's branch name
   */
  parentBranch: string;

  /**
   * Callback when user chooses to recreate the parent worktree
   */
  onRecreateWorktree: () => void;

  /**
   * Callback when user chooses to work in a new worktree
   */
  onNewWorktree: () => void;

  /**
   * Whether an action is currently in progress
   */
  isLoading?: boolean;
}

/**
 * ParentWorktreeDialog - Dialog shown when parent task's worktree is archived/removed
 *
 * Presents the user with two options:
 * 1. Re-create the parent worktree - Restores the original worktree for the subtask
 * 2. Work in new worktree - Creates a fresh worktree for this subtask
 *
 * This dialog is shown when:
 * - User tries to execute a subtask with "use parent worktree" enabled
 * - The parent task's worktree has been archived or removed
 */
export function ParentWorktreeDialog({
  open,
  onOpenChange,
  parentBranch,
  onRecreateWorktree,
  onNewWorktree,
  isLoading = false,
}: ParentWorktreeDialogProps) {
  const { t } = useTranslation(['tasks', 'common']);

  const handleBackdropClick = (e: React.MouseEvent) => {
    if (e.target === e.currentTarget && !isLoading) {
      onOpenChange(false);
    }
  };

  // Handle escape key
  React.useEffect(() => {
    if (!open || isLoading) return;

    const handleEscape = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        onOpenChange(false);
      }
    };

    document.addEventListener('keydown', handleEscape);
    return () => document.removeEventListener('keydown', handleEscape);
  }, [open, isLoading, onOpenChange]);

  return (
    <AnimatePresence>
      {open && (
        <>
          {/* Backdrop */}
          <motion.div
            className="fixed inset-0 z-[10000] bg-black/50"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            onClick={handleBackdropClick}
          />

          {/* Dialog */}
          <motion.div
            className={cn(
              'fixed z-[10001] bg-background rounded-lg shadow-xl p-6',
              'left-1/2 top-1/2 -translate-x-1/2 -translate-y-1/2',
              'w-[min(90vw,400px)]'
            )}
            initial={{ opacity: 0, scale: 0.95 }}
            animate={{ opacity: 1, scale: 1 }}
            exit={{ opacity: 0, scale: 0.95 }}
            transition={{ duration: 0.2 }}
          >
            <div className="space-y-4">
              {/* Header */}
              <div className="text-center">
                <h3 className="text-lg font-semibold">
                  {t('parentWorktreeDialog.title', 'Parent Worktree')}
                </h3>
              </div>

              {/* Description */}
              <div className="text-sm text-muted-foreground text-center space-y-2">
                <p>
                  {t(
                    'parentWorktreeDialog.description',
                    "The parent task's worktree has been archived or removed."
                  )}
                </p>
                <p>
                  {t(
                    'parentWorktreeDialog.question',
                    'How would you like to proceed?'
                  )}
                </p>
              </div>

              {/* Options */}
              <div className="space-y-3">
                {/* Option 1: Recreate parent worktree */}
                <Button
                  variant="outline"
                  className="w-full h-auto py-3 px-4 justify-start"
                  onClick={onRecreateWorktree}
                  disabled={isLoading}
                >
                  <RefreshCw className={cn('h-5 w-5 mr-3', isLoading && 'animate-spin')} />
                  <div className="text-left">
                    <div className="font-medium">
                      {t('parentWorktreeDialog.recreateOption', 'Re-create parent worktree')}
                    </div>
                    <div className="text-xs text-muted-foreground mt-0.5">
                      {t(
                        'parentWorktreeDialog.recreateDescription',
                        'Recreates {{branch}}',
                        { branch: parentBranch }
                      )}
                    </div>
                  </div>
                </Button>

                {/* Option 2: Work in new worktree */}
                <Button
                  variant="outline"
                  className="w-full h-auto py-3 px-4 justify-start"
                  onClick={onNewWorktree}
                  disabled={isLoading}
                >
                  <FolderPlus className="h-5 w-5 mr-3" />
                  <div className="text-left">
                    <div className="font-medium">
                      {t('parentWorktreeDialog.newWorktreeOption', 'Work in new worktree')}
                    </div>
                    <div className="text-xs text-muted-foreground mt-0.5">
                      {t(
                        'parentWorktreeDialog.newWorktreeDescription',
                        'Creates a new worktree for this task'
                      )}
                    </div>
                  </div>
                </Button>
              </div>

              {/* Cancel button */}
              <div className="pt-2">
                <Button
                  variant="ghost"
                  className="w-full"
                  onClick={() => onOpenChange(false)}
                  disabled={isLoading}
                >
                  {t('common:cancel', 'Cancel')}
                </Button>
              </div>
            </div>
          </motion.div>
        </>
      )}
    </AnimatePresence>
  );
}

export default ParentWorktreeDialog;
