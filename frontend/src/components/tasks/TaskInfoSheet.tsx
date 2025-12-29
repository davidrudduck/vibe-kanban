import * as React from 'react';
import { motion, AnimatePresence, PanInfo } from 'framer-motion';
import { ChevronRight, X } from 'lucide-react';
import { cn } from '@/lib/utils';
import { Button } from '@/components/ui/button';
import MarkdownRenderer from '@/components/ui/markdown-renderer';
import type { TaskWithAttemptStatus } from 'shared/types';

interface TaskInfoSheetProps {
  task: TaskWithAttemptStatus;
  isOpen: boolean;
  onOpenChange: (open: boolean) => void;
  relationships?: React.ReactNode;
  variables?: React.ReactNode;
  className?: string;
}

/**
 * Swipe-down overlay for task metadata on mobile.
 * Shows task description, relationships, and variables.
 * Animated with framer-motion (slides from top).
 */
export function TaskInfoSheet({
  task,
  isOpen,
  onOpenChange,
  relationships,
  variables,
  className,
}: TaskInfoSheetProps) {
  // Close on escape key
  React.useEffect(() => {
    if (!isOpen) return;

    const handleEscape = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        onOpenChange(false);
      }
    };

    document.addEventListener('keydown', handleEscape);
    return () => document.removeEventListener('keydown', handleEscape);
  }, [isOpen, onOpenChange]);

  // Handle drag to dismiss
  const handleDragEnd = (_: unknown, info: PanInfo) => {
    // If dragged down more than 100px or velocity is high, close
    if (info.offset.y > 100 || info.velocity.y > 500) {
      onOpenChange(false);
    }
  };

  return (
    <AnimatePresence>
      {isOpen && (
        <>
          {/* Backdrop overlay */}
          <motion.div
            key="backdrop"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={{ duration: 0.2 }}
            className="fixed inset-0 bg-black/50 z-40"
            onClick={() => onOpenChange(false)}
            aria-hidden="true"
          />

          {/* Sheet */}
          <motion.div
            key="sheet"
            initial={{ y: '-100%' }}
            animate={{ y: 0 }}
            exit={{ y: '-100%' }}
            transition={{ type: 'spring', damping: 25, stiffness: 300 }}
            drag="y"
            dragConstraints={{ top: 0, bottom: 0 }}
            dragElastic={{ top: 0.1, bottom: 0.5 }}
            onDragEnd={handleDragEnd}
            className={cn(
              'fixed top-0 left-0 right-0 z-50 bg-background border-b rounded-b-xl shadow-lg max-h-[80vh] overflow-hidden flex flex-col',
              className
            )}
          >
            {/* Drag handle */}
            <div className="flex justify-center py-2">
              <div className="w-10 h-1 rounded-full bg-muted-foreground/30" />
            </div>

            {/* Header */}
            <div className="flex items-center justify-between px-4 pb-2 border-b">
              <h2 className="text-sm font-semibold">Task Info</h2>
              <Button
                variant="ghost"
                size="icon"
                className="h-8 w-8"
                onClick={() => onOpenChange(false)}
                aria-label="Close"
              >
                <X className="h-4 w-4" />
              </Button>
            </div>

            {/* Content */}
            <div className="flex-1 overflow-y-auto p-4 space-y-4">
              {/* Description */}
              {task.description && (
                <div>
                  <h3 className="text-xs font-medium text-muted-foreground uppercase tracking-wide mb-2">
                    Description
                  </h3>
                  <div className="text-sm prose prose-sm dark:prose-invert max-w-none">
                    <MarkdownRenderer content={task.description} />
                  </div>
                </div>
              )}

              {/* Relationships */}
              {relationships && (
                <div>
                  <h3 className="text-xs font-medium text-muted-foreground uppercase tracking-wide mb-2">
                    Relationships
                  </h3>
                  {relationships}
                </div>
              )}

              {/* Variables */}
              {variables && (
                <div>
                  <h3 className="text-xs font-medium text-muted-foreground uppercase tracking-wide mb-2">
                    Variables
                  </h3>
                  {variables}
                </div>
              )}

              {/* Empty state */}
              {!task.description && !relationships && !variables && (
                <p className="text-sm text-muted-foreground text-center py-8">
                  No additional task information available.
                </p>
              )}
            </div>
          </motion.div>
        </>
      )}
    </AnimatePresence>
  );
}

/**
 * Trigger button to open the TaskInfoSheet.
 * Shows a collapsed indicator when info is available.
 */
interface TaskInfoTriggerProps {
  hasInfo: boolean;
  isOpen: boolean;
  onClick: () => void;
  className?: string;
}

export function TaskInfoTrigger({
  hasInfo,
  isOpen,
  onClick,
  className,
}: TaskInfoTriggerProps) {
  if (!hasInfo) return null;

  return (
    <Button
      variant="ghost"
      size="sm"
      className={cn('h-6 px-2 text-xs', className)}
      onClick={onClick}
      aria-expanded={isOpen}
      aria-label={isOpen ? 'Collapse task info' : 'Expand task info'}
    >
      <ChevronRight
        className={cn(
          'h-3 w-3 transition-transform',
          isOpen && 'rotate-90'
        )}
      />
      <span className="ml-1">Info</span>
    </Button>
  );
}

export default TaskInfoSheet;
