import { cn } from '@/lib/utils';
import { LabelBadge } from '@/components/labels/LabelBadge';
import type { Label } from 'shared/types';
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from '@/components/ui/tooltip';

interface CompactLabelListProps {
  /** Array of labels to display */
  labels: Label[] | undefined;
  /** Maximum number of labels to show before collapsing (default: 2) */
  maxVisible?: number;
  /** Size of the label badges */
  size?: 'sm' | 'md';
  /** Additional CSS classes */
  className?: string;
}

/**
 * Compact label list that shows a limited number of labels with an overflow indicator.
 * Shows first N labels as LabelBadge components, then a "+N" badge for remaining.
 * The overflow badge has a tooltip listing the hidden label names.
 */
export function CompactLabelList({
  labels,
  maxVisible = 2,
  size = 'sm',
  className,
}: CompactLabelListProps) {
  if (!labels || labels.length === 0) return null;

  const visibleLabels = labels.slice(0, maxVisible);
  const hiddenLabels = labels.slice(maxVisible);
  const hiddenCount = hiddenLabels.length;

  return (
    <div className={cn('flex flex-wrap items-center gap-1', className)}>
      {visibleLabels.map((label) => (
        <LabelBadge key={label.id} label={label} size={size} />
      ))}
      {hiddenCount > 0 && (
        <TooltipProvider>
          <Tooltip>
            <TooltipTrigger asChild>
              <span
                className={cn(
                  'inline-flex items-center rounded-full font-medium',
                  'bg-muted text-muted-foreground cursor-default',
                  size === 'sm' ? 'text-xs px-1.5 py-0.5' : 'text-sm px-2 py-1'
                )}
              >
                +{hiddenCount}
              </span>
            </TooltipTrigger>
            <TooltipContent side="top" className="max-w-[200px]">
              <div className="flex flex-col gap-1">
                {hiddenLabels.map((label) => (
                  <span key={label.id} className="text-xs">
                    {label.name}
                  </span>
                ))}
              </div>
            </TooltipContent>
          </Tooltip>
        </TooltipProvider>
      )}
    </div>
  );
}
