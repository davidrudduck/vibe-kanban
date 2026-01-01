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
import type { SwarmTemplate } from '@/types/swarm';

interface MergeTemplatesDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  templates: SwarmTemplate[];
  targetTemplate: SwarmTemplate;
  onMerge: (sourceId: string) => Promise<void>;
  isMerging: boolean;
}

export function MergeTemplatesDialog({
  open,
  onOpenChange,
  templates,
  targetTemplate,
  onMerge,
  isMerging,
}: MergeTemplatesDialogProps) {
  const { t } = useTranslation(['settings', 'common']);
  const [sourceId, setSourceId] = useState<string>('');
  const [error, setError] = useState<string | null>(null);

  // Filter out the target template from available sources
  const availableSources = useMemo(
    () => templates.filter((t) => t.id !== targetTemplate.id),
    [templates, targetTemplate.id]
  );

  const selectedSource = availableSources.find((t) => t.id === sourceId);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError(null);

    if (!sourceId) {
      setError(
        t(
          'settings.swarm.templates.merge.sourceRequired',
          'Please select a template to merge'
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

  // Helper to truncate content for display
  const truncateContent = (content: string, maxLength = 50) => {
    if (content.length <= maxLength) return content;
    return content.substring(0, maxLength).trim() + '...';
  };

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent className="sm:max-w-md">
        <form onSubmit={handleSubmit}>
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              <GitMerge className="h-5 w-5" />
              {t('settings.swarm.templates.merge.title', 'Merge Templates')}
            </DialogTitle>
            <DialogDescription>
              {t(
                'settings.swarm.templates.merge.description',
                'Merge another template into the selected target. The source template will be deleted.'
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
                    'settings.swarm.templates.merge.noOtherTemplates',
                    'No other templates available to merge.'
                  )}
                </AlertDescription>
              </Alert>
            ) : (
              <>
                <div className="space-y-2">
                  <Label htmlFor="source-template">
                    {t(
                      'settings.swarm.templates.merge.sourceLabel',
                      'Merge from'
                    )}
                  </Label>
                  <Select value={sourceId} onValueChange={setSourceId}>
                    <SelectTrigger id="source-template">
                      <SelectValue
                        placeholder={t(
                          'settings.swarm.templates.merge.selectSource',
                          'Select a template to merge...'
                        )}
                      />
                    </SelectTrigger>
                    <SelectContent>
                      {availableSources.map((template) => (
                        <SelectItem key={template.id} value={template.id}>
                          <span className="flex flex-col items-start">
                            <span className="font-medium">
                              @{template.name}
                            </span>
                            <span className="text-xs text-muted-foreground">
                              {truncateContent(template.content)}
                            </span>
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
                        'settings.swarm.templates.merge.warning',
                        '"@{{source}}" will be deleted after merging. This action cannot be undone.',
                        { source: selectedSource.name }
                      )}
                    </AlertDescription>
                  </Alert>
                )}

                <div className="space-y-2">
                  <Label>
                    {t(
                      'settings.swarm.templates.merge.targetLabel',
                      'Merge into'
                    )}
                  </Label>
                  <div className="flex flex-col gap-1 px-3 py-2 bg-muted rounded-md">
                    <span className="font-medium">@{targetTemplate.name}</span>
                    <span className="text-xs text-muted-foreground">
                      {truncateContent(targetTemplate.content)}
                    </span>
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
              {t('settings.swarm.templates.merge.confirm', 'Merge Templates')}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
