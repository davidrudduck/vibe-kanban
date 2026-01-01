import { useState, useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import { Loader2, GitMerge, AlertTriangle } from 'lucide-react';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Label } from '@/components/ui/label';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { LabelBadge } from '@/components/labels/LabelBadge';
import type { SwarmLabel } from '@/types/swarm';
import type { Label as LabelType } from 'shared/types';

// Helper to convert SwarmLabel to local Label format for LabelBadge
function swarmLabelToLabel(swarmLabel: SwarmLabel): LabelType {
  return {
    id: swarmLabel.id,
    name: swarmLabel.name,
    color: swarmLabel.color,
    icon: swarmLabel.icon || 'tag', // Default icon if null
    project_id: swarmLabel.project_id,
    version: BigInt(1),
    created_at: swarmLabel.created_at,
    updated_at: swarmLabel.updated_at,
  };
}

interface MergeLabelsDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  labels: SwarmLabel[];
  targetLabel: SwarmLabel;
  onMerge: (sourceId: string) => Promise<void>;
  isMerging: boolean;
}

export function MergeLabelsDialog({
  open,
  onOpenChange,
  labels,
  targetLabel,
  onMerge,
  isMerging,
}: MergeLabelsDialogProps) {
  const { t } = useTranslation(['settings', 'common']);
  const [sourceId, setSourceId] = useState<string>('');
  const [error, setError] = useState<string | null>(null);

  // Filter out the target label from available sources
  const availableSources = useMemo(
    () => labels.filter((l) => l.id !== targetLabel.id),
    [labels, targetLabel.id]
  );

  const selectedSource = availableSources.find((l) => l.id === sourceId);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError(null);

    if (!sourceId) {
      setError(
        t(
          'settings.swarm.labels.merge.sourceRequired',
          'Please select a label to merge'
        )
      );
      return;
    }

    try {
      await onMerge(sourceId);
      setSourceId('');
      onOpenChange(false);
    } catch (err) {
      const message = err instanceof Error ? err.message : 'An error occurred';
      setError(message);
    }
  };

  const handleOpenChange = (isOpen: boolean) => {
    if (!isOpen) {
      setSourceId('');
      setError(null);
    }
    onOpenChange(isOpen);
  };

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent className="sm:max-w-md">
        <form onSubmit={handleSubmit}>
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              <GitMerge className="h-5 w-5" />
              {t('settings.swarm.labels.merge.title', 'Merge Labels')}
            </DialogTitle>
            <DialogDescription>
              {t(
                'settings.swarm.labels.merge.description',
                'Merge another label into the selected target. All tasks using the source label will be updated to use the target label.'
              )}
            </DialogDescription>
          </DialogHeader>

          <div className="space-y-4 py-4">
            {error && (
              <Alert variant="destructive">
                <AlertDescription>{error}</AlertDescription>
              </Alert>
            )}

            {availableSources.length === 0 ? (
              <Alert>
                <AlertDescription>
                  {t(
                    'settings.swarm.labels.merge.noOtherLabels',
                    'No other labels available to merge.'
                  )}
                </AlertDescription>
              </Alert>
            ) : (
              <>
                <div className="space-y-2">
                  <Label htmlFor="source-label">
                    {t('settings.swarm.labels.merge.sourceLabel', 'Merge from')}
                  </Label>
                  <Select value={sourceId} onValueChange={setSourceId}>
                    <SelectTrigger id="source-label">
                      <SelectValue
                        placeholder={t(
                          'settings.swarm.labels.merge.selectSource',
                          'Select a label to merge...'
                        )}
                      />
                    </SelectTrigger>
                    <SelectContent>
                      {availableSources.map((label) => (
                        <SelectItem key={label.id} value={label.id}>
                          <span className="flex items-center gap-2">
                            <LabelBadge
                              label={swarmLabelToLabel(label)}
                              size="sm"
                            />
                          </span>
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                </div>

                {selectedSource && (
                  <Alert className="border-amber-500/50 bg-amber-50 text-amber-900 dark:bg-amber-900/20 dark:text-amber-200 [&>svg]:text-amber-500">
                    <AlertTriangle className="h-4 w-4" />
                    <AlertDescription>
                      {t(
                        'settings.swarm.labels.merge.warning',
                        '"{{source}}" will be deleted after merging. All tasks with this label will be updated to use the target label.',
                        { source: selectedSource.name }
                      )}
                    </AlertDescription>
                  </Alert>
                )}

                <div className="space-y-2">
                  <Label>
                    {t('settings.swarm.labels.merge.targetLabel', 'Merge into')}
                  </Label>
                  <div className="flex items-center gap-2 px-3 py-2 bg-muted rounded-md">
                    <LabelBadge
                      label={swarmLabelToLabel(targetLabel)}
                      size="md"
                    />
                  </div>
                </div>
              </>
            )}
          </div>

          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              onClick={() => handleOpenChange(false)}
              disabled={isMerging}
            >
              {t('common:cancel', 'Cancel')}
            </Button>
            <Button
              type="submit"
              disabled={isMerging || !sourceId || availableSources.length === 0}
              variant="destructive"
            >
              {isMerging && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
              {t('settings.swarm.labels.merge.confirm', 'Merge Labels')}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
