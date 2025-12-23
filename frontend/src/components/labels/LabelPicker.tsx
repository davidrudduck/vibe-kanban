import { useState, useCallback, useEffect } from 'react';
import { Button } from '@/components/ui/button';
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from '@/components/ui/popover';
import { Check, Plus, Tag } from 'lucide-react';
import { labelsApi } from '@/lib/api';
import { LabelBadge } from './LabelBadge';
import { cn } from '@/lib/utils';
import type { Label } from 'shared/types';

interface LabelPickerProps {
  taskId: string;
  projectId?: string;
  selectedLabels: Label[];
  onLabelsChange: (labels: Label[]) => void;
  disabled?: boolean;
  className?: string;
}

export function LabelPicker({
  taskId,
  projectId,
  selectedLabels,
  onLabelsChange,
  disabled,
  className,
}: LabelPickerProps) {
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

  const toggleLabel = async (label: Label) => {
    const isSelected = selectedLabels.some((l) => l.id === label.id);
    let newLabels: Label[];

    if (isSelected) {
      newLabels = selectedLabels.filter((l) => l.id !== label.id);
    } else {
      newLabels = [...selectedLabels, label];
    }

    // Optimistically update UI
    onLabelsChange(newLabels);

    // Persist to backend
    try {
      await labelsApi.setTaskLabels(taskId, {
        label_ids: newLabels.map((l) => l.id),
      });
    } catch (err) {
      console.error('Failed to update task labels:', err);
      // Revert on error
      onLabelsChange(selectedLabels);
    }
  };

  const removeLabel = async (label: Label) => {
    const newLabels = selectedLabels.filter((l) => l.id !== label.id);
    onLabelsChange(newLabels);

    try {
      await labelsApi.setTaskLabels(taskId, {
        label_ids: newLabels.map((l) => l.id),
      });
    } catch (err) {
      console.error('Failed to remove label:', err);
      onLabelsChange(selectedLabels);
    }
  };

  return (
    <div className={cn('flex flex-wrap items-center gap-1.5', className)}>
      {/* Display selected labels */}
      {selectedLabels.map((label) => (
        <LabelBadge
          key={label.id}
          label={label}
          size="sm"
          onRemove={disabled ? undefined : () => removeLabel(label)}
        />
      ))}

      {/* Add label button */}
      {!disabled && (
        <Popover open={open} onOpenChange={setOpen}>
          <PopoverTrigger asChild>
            <Button
              variant="ghost"
              size="sm"
              className="h-6 px-2 text-xs text-muted-foreground hover:text-foreground"
            >
              <Plus className="h-3 w-3 mr-1" />
              <Tag className="h-3 w-3" />
            </Button>
          </PopoverTrigger>
          <PopoverContent className="w-56 p-2" align="start">
            {loading ? (
              <div className="py-4 text-center text-sm text-muted-foreground">
                Loading...
              </div>
            ) : availableLabels.length === 0 ? (
              <div className="py-4 text-center text-sm text-muted-foreground">
                No labels available
              </div>
            ) : (
              <div className="max-h-48 overflow-y-auto">
                <div className="space-y-1">
                  {availableLabels.map((label) => {
                    const isSelected = selectedLabels.some(
                      (l) => l.id === label.id
                    );
                    return (
                      <button
                        key={label.id}
                        type="button"
                        className={cn(
                          'flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-sm transition-colors hover:bg-accent',
                          isSelected && 'bg-accent'
                        )}
                        onClick={() => toggleLabel(label)}
                      >
                        <LabelBadge label={label} size="sm" />
                        <span className="flex-1" />
                        {isSelected && (
                          <Check className="h-4 w-4 text-foreground" />
                        )}
                      </button>
                    );
                  })}
                </div>
              </div>
            )}
          </PopoverContent>
        </Popover>
      )}
    </div>
  );
}
