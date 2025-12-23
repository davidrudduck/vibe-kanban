import { useState, useCallback, useEffect } from 'react';
import { Button } from '@/components/ui/button';
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from '@/components/ui/popover';
import { Check, Tag, ChevronDown } from 'lucide-react';
import { labelsApi } from '@/lib/api';
import { LabelBadge } from './LabelBadge';
import { cn } from '@/lib/utils';
import type { Label } from 'shared/types';

interface LabelSelectProps {
  projectId?: string;
  selectedLabel: Label | null;
  onLabelChange: (label: Label | null) => void;
  disabled?: boolean;
  className?: string;
}

/**
 * Compact single-select dropdown for labels.
 * Used in the TaskFormDialog action bar for both create and edit modes.
 */
export function LabelSelect({
  projectId,
  selectedLabel,
  onLabelChange,
  disabled,
  className,
}: LabelSelectProps) {
  const [open, setOpen] = useState(false);
  const [availableLabels, setAvailableLabels] = useState<Label[]>([]);
  const [loading, setLoading] = useState(false);

  // Fetch available labels when popover opens
  const fetchLabels = useCallback(async () => {
    setLoading(true);
    try {
      const labels = await labelsApi.list(
        projectId ? { project_id: projectId } : undefined
      );
      setAvailableLabels(labels);
    } catch (err) {
      console.error('Failed to fetch labels:', err);
    } finally {
      setLoading(false);
    }
  }, [projectId]);

  useEffect(() => {
    if (open) {
      fetchLabels();
    }
  }, [open, fetchLabels]);

  const selectLabel = (label: Label | null) => {
    onLabelChange(label);
    setOpen(false);
  };

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <Button
          variant="outline"
          size="sm"
          className={cn(
            'h-9 gap-1.5 px-2.5 min-w-[100px] justify-between',
            className
          )}
          disabled={disabled}
        >
          {selectedLabel ? (
            <LabelBadge label={selectedLabel} size="sm" />
          ) : (
            <span className="flex items-center gap-1.5 text-muted-foreground">
              <Tag className="h-3.5 w-3.5" />
              <span className="text-xs">Label</span>
            </span>
          )}
          <ChevronDown className="h-3 w-3 opacity-50" />
        </Button>
      </PopoverTrigger>
      <PopoverContent className="w-48 p-1.5" align="end">
        {loading ? (
          <div className="py-3 text-center text-sm text-muted-foreground">
            Loading...
          </div>
        ) : (
          <div className="max-h-48 overflow-y-auto">
            {/* No label option */}
            <button
              type="button"
              className={cn(
                'flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-sm transition-colors hover:bg-accent',
                !selectedLabel && 'bg-accent'
              )}
              onClick={() => selectLabel(null)}
            >
              <span className="flex items-center gap-1.5 text-muted-foreground">
                <Tag className="h-3.5 w-3.5" />
                <span>No label</span>
              </span>
              <span className="flex-1" />
              {!selectedLabel && (
                <Check className="h-4 w-4 text-foreground" />
              )}
            </button>

            {/* Separator */}
            {availableLabels.length > 0 && (
              <div className="my-1 h-px bg-border" />
            )}

            {/* Label options */}
            {availableLabels.map((label) => {
              const isSelected = selectedLabel?.id === label.id;
              return (
                <button
                  key={label.id}
                  type="button"
                  className={cn(
                    'flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-sm transition-colors hover:bg-accent',
                    isSelected && 'bg-accent'
                  )}
                  onClick={() => selectLabel(label)}
                >
                  <LabelBadge label={label} size="sm" />
                  <span className="flex-1" />
                  {isSelected && (
                    <Check className="h-4 w-4 text-foreground" />
                  )}
                </button>
              );
            })}

            {availableLabels.length === 0 && (
              <div className="py-2 text-center text-xs text-muted-foreground">
                No labels available
              </div>
            )}
          </div>
        )}
      </PopoverContent>
    </Popover>
  );
}
