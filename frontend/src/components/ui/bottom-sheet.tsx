import * as React from 'react';
import { motion, AnimatePresence, PanInfo, useDragControls } from 'framer-motion';
import { cn } from '@/lib/utils';
import { useMediaQuery } from '@/hooks/useMediaQuery';
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from '@/components/ui/popover';

interface BottomSheetProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  children: React.ReactNode;
  trigger?: React.ReactNode;
  title?: string;
  className?: string;
}

const DRAG_CLOSE_THRESHOLD = 100;

/**
 * BottomSheet component - Mobile-first responsive sheet
 *
 * On mobile (< 768px): Slides up from bottom with drag-to-dismiss
 * On desktop (>= 768px): Falls back to Popover
 */
function BottomSheet({
  open,
  onOpenChange,
  children,
  trigger,
  title,
  className,
}: BottomSheetProps) {
  const isDesktop = useMediaQuery('(min-width: 768px)');
  const dragControls = useDragControls();
  const sheetRef = React.useRef<HTMLDivElement>(null);

  // Handle drag end to determine if should close
  const handleDragEnd = (
    _event: MouseEvent | TouchEvent | PointerEvent,
    info: PanInfo
  ) => {
    // Close if dragged down past threshold or with high velocity
    if (info.offset.y > DRAG_CLOSE_THRESHOLD || info.velocity.y > 500) {
      onOpenChange(false);
    }
  };

  // Handle backdrop click
  const handleBackdropClick = (e: React.MouseEvent) => {
    if (e.target === e.currentTarget) {
      onOpenChange(false);
    }
  };

  // Handle escape key
  React.useEffect(() => {
    if (!open || isDesktop) return;

    const handleEscape = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        onOpenChange(false);
      }
    };

    document.addEventListener('keydown', handleEscape);
    return () => document.removeEventListener('keydown', handleEscape);
  }, [open, isDesktop, onOpenChange]);

  // Prevent body scroll when sheet is open on mobile
  React.useEffect(() => {
    if (!open || isDesktop) return;

    const originalOverflow = document.body.style.overflow;
    document.body.style.overflow = 'hidden';
    return () => {
      document.body.style.overflow = originalOverflow;
    };
  }, [open, isDesktop]);

  // Desktop: Use Popover
  if (isDesktop) {
    return (
      <Popover open={open} onOpenChange={onOpenChange}>
        {trigger && <PopoverTrigger asChild>{trigger}</PopoverTrigger>}
        <PopoverContent
          className={cn('w-72', className)}
          align="start"
          sideOffset={8}
        >
          {title && (
            <div className="font-medium text-sm mb-3 text-foreground">
              {title}
            </div>
          )}
          {children}
        </PopoverContent>
      </Popover>
    );
  }

  // Mobile: Use animated bottom sheet
  return (
    <>
      {trigger &&
        React.cloneElement(trigger as React.ReactElement, {
          onClick: () => onOpenChange(!open),
        })}

      <AnimatePresence>
        {open && (
          <>
            {/* Backdrop */}
            <motion.div
              className="fixed inset-0 z-50 bg-black/50"
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              exit={{ opacity: 0 }}
              onClick={handleBackdropClick}
            />

            {/* Sheet */}
            <motion.div
              ref={sheetRef}
              className={cn(
                'fixed bottom-0 left-0 right-0 z-50 bg-popover text-popover-foreground rounded-t-xl shadow-lg',
                'max-h-[85vh] overflow-hidden',
                className
              )}
              initial={{ y: '100%' }}
              animate={{ y: 0 }}
              exit={{ y: '100%' }}
              transition={{ type: 'spring', damping: 30, stiffness: 300 }}
              drag="y"
              dragControls={dragControls}
              dragConstraints={{ top: 0, bottom: 0 }}
              dragElastic={{ top: 0, bottom: 0.5 }}
              onDragEnd={handleDragEnd}
            >
              {/* Drag handle */}
              <div
                className="flex justify-center py-3 cursor-grab active:cursor-grabbing touch-none"
                onPointerDown={(e) => dragControls.start(e)}
              >
                <div className="w-10 h-1 bg-muted-foreground/30 rounded-full" />
              </div>

              {/* Content */}
              <div className="px-4 pb-8 overflow-y-auto max-h-[calc(85vh-44px)]">
                {title && (
                  <div className="font-medium text-base mb-4 text-foreground">
                    {title}
                  </div>
                )}
                {children}
              </div>
            </motion.div>
          </>
        )}
      </AnimatePresence>
    </>
  );
}

BottomSheet.displayName = 'BottomSheet';

export { BottomSheet };
export type { BottomSheetProps };
