import { useState, useCallback } from 'react';
import { Archive, ArchiveRestore } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from '@/components/ui/tooltip';
import { Button } from '@/components/ui/button';
import { cn } from '@/lib/utils';

interface ArchiveToggleIconProps {
  isArchived: boolean;
  onArchive: () => void;
  onUnarchive: () => void;
  disabled?: boolean;
  className?: string;
}

export function ArchiveToggleIcon({
  isArchived,
  onArchive,
  onUnarchive,
  disabled = false,
  className,
}: ArchiveToggleIconProps) {
  const { t } = useTranslation('tasks');
  const [isHovered, setIsHovered] = useState(false);

  const handleClick = useCallback(
    (e: React.MouseEvent) => {
      e.stopPropagation();
      if (disabled) return;

      if (isArchived) {
        onUnarchive();
      } else {
        onArchive();
      }
    },
    [isArchived, onArchive, onUnarchive, disabled]
  );

  const handlePointerDown = useCallback((e: React.PointerEvent) => {
    e.stopPropagation();
  }, []);

  const handleMouseDown = useCallback((e: React.MouseEvent) => {
    e.stopPropagation();
  }, []);

  const Icon = isArchived ? ArchiveRestore : Archive;
  const tooltipText = isArchived
    ? t('actionsMenu.unarchive')
    : t('actionsMenu.archive');

  return (
    <TooltipProvider>
      <Tooltip>
        <TooltipTrigger asChild>
          <Button
            variant="icon"
            onClick={handleClick}
            onPointerDown={handlePointerDown}
            onMouseDown={handleMouseDown}
            onMouseEnter={() => setIsHovered(true)}
            onMouseLeave={() => setIsHovered(false)}
            disabled={disabled}
            aria-label={tooltipText}
            className={cn(
              'h-6 w-6 min-h-6 min-w-6 p-1',
              !isHovered && 'text-muted-foreground',
              isHovered && !disabled && 'text-foreground',
              className
            )}
          >
            <Icon className="h-4 w-4" />
          </Button>
        </TooltipTrigger>
        <TooltipContent side="top">{tooltipText}</TooltipContent>
      </Tooltip>
    </TooltipProvider>
  );
}
