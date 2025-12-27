import { ChevronLeft, ChevronRight } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { cn } from '@/lib/utils';

interface MobileColumnHeaderProps {
  /** Column name to display */
  name: string;
  /** Number of tasks in the column */
  count: number;
  /** Color CSS variable for the column indicator */
  color: string;
  /** Whether this is the first column (hides left arrow) */
  isFirst: boolean;
  /** Whether this is the last column (hides right arrow) */
  isLast: boolean;
  /** Callback when left arrow is clicked */
  onPrev: () => void;
  /** Callback when right arrow is clicked */
  onNext: () => void;
  /** Current column index (0-based) */
  currentIndex: number;
  /** Total number of columns */
  totalColumns: number;
  className?: string;
}

/**
 * Mobile-optimized column header with navigation arrows and indicator dots.
 * Shows column name, task count, and navigation between columns.
 */
function MobileColumnHeader({
  name,
  count,
  color,
  isFirst,
  isLast,
  onPrev,
  onNext,
  currentIndex,
  totalColumns,
  className,
}: MobileColumnHeaderProps) {
  return (
    <div
      className={cn(
        'flex flex-col items-center px-4 py-3 bg-background border-b',
        className
      )}
      style={{
        backgroundImage: `linear-gradient(hsl(var(${color}) / 0.05), hsl(var(${color}) / 0.05))`,
      }}
    >
      {/* Navigation row */}
      <div className="flex items-center justify-between w-full">
        <Button
          variant="ghost"
          size="icon"
          className="h-10 w-10"
          onClick={onPrev}
          disabled={isFirst}
          aria-label="Previous column"
          data-testid="prev-column-btn"
        >
          <ChevronLeft
            className={cn('h-6 w-6', isFirst && 'text-muted-foreground/30')}
          />
        </Button>

        <div className="flex items-center gap-2">
          <div
            className="h-3 w-3 rounded-full"
            style={{ backgroundColor: `hsl(var(${color}))` }}
            aria-hidden="true"
          />
          <span className="text-base font-medium">{name}</span>
          <span className="text-sm text-muted-foreground">({count})</span>
        </div>

        <Button
          variant="ghost"
          size="icon"
          className="h-10 w-10"
          onClick={onNext}
          disabled={isLast}
          aria-label="Next column"
          data-testid="next-column-btn"
        >
          <ChevronRight
            className={cn('h-6 w-6', isLast && 'text-muted-foreground/30')}
          />
        </Button>
      </div>

      {/* Indicator dots */}
      <div
        className="flex gap-1.5 mt-2"
        role="tablist"
        aria-label="Column indicators"
      >
        {Array.from({ length: totalColumns }).map((_, index) => (
          <div
            key={index}
            className={cn(
              'h-1.5 w-1.5 rounded-full transition-colors',
              index === currentIndex
                ? 'bg-foreground'
                : 'bg-muted-foreground/30'
            )}
            role="tab"
            aria-selected={index === currentIndex}
            aria-label={`Column ${index + 1} of ${totalColumns}`}
          />
        ))}
      </div>
    </div>
  );
}

export default MobileColumnHeader;
