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
import type { SwarmProjectWithNodes } from '@/types/swarm';

interface MergeProjectsDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  projects: SwarmProjectWithNodes[];
  targetProject: SwarmProjectWithNodes;
  onMerge: (sourceId: string) => Promise<void>;
  isMerging: boolean;
}

export function MergeProjectsDialog({
  open,
  onOpenChange,
  projects,
  targetProject,
  onMerge,
  isMerging,
}: MergeProjectsDialogProps) {
  const { t } = useTranslation(['settings', 'common']);
  const [sourceId, setSourceId] = useState<string>('');
  const [error, setError] = useState<string | null>(null);

  // Filter out the target project from available sources
  const availableSources = useMemo(
    () => projects.filter((p) => p.id !== targetProject.id),
    [projects, targetProject.id]
  );

  const selectedSource = availableSources.find((p) => p.id === sourceId);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError(null);

    if (!sourceId) {
      setError(
        t(
          'settings.swarm.merge.sourceRequired',
          'Please select a project to merge'
        )
      );
      return;
    }

    try {
      await onMerge(sourceId);
      setSourceId('');
      onOpenChange(false);
    } catch (err) {
      const message =
        err instanceof Error ? err.message : 'An error occurred';
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
              {t('settings.swarm.merge.title', 'Merge Projects')}
            </DialogTitle>
            <DialogDescription>
              {t(
                'settings.swarm.merge.description',
                'Merge another project into "{{target}}". All linked nodes will be transferred and the source project will be deleted.',
                { target: targetProject.name }
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
                    'settings.swarm.merge.noOtherProjects',
                    'No other projects available to merge.'
                  )}
                </AlertDescription>
              </Alert>
            ) : (
              <>
                <div className="space-y-2">
                  <Label htmlFor="source-project">
                    {t('settings.swarm.merge.sourceLabel', 'Merge from')}
                  </Label>
                  <Select value={sourceId} onValueChange={setSourceId}>
                    <SelectTrigger id="source-project">
                      <SelectValue
                        placeholder={t(
                          'settings.swarm.merge.selectSource',
                          'Select a project to merge...'
                        )}
                      />
                    </SelectTrigger>
                    <SelectContent>
                      {availableSources.map((project) => (
                        <SelectItem key={project.id} value={project.id}>
                          <span className="flex items-center gap-2">
                            <span>{project.name}</span>
                            <span className="text-muted-foreground text-xs">
                              ({project.linked_nodes_count}{' '}
                              {project.linked_nodes_count === 1
                                ? 'node'
                                : 'nodes'}
                              )
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
                        'settings.swarm.merge.warning',
                        '"{{source}}" will be deleted after merging. This action cannot be undone.',
                        { source: selectedSource.name }
                      )}
                    </AlertDescription>
                  </Alert>
                )}

                <div className="space-y-2">
                  <Label>
                    {t('settings.swarm.merge.targetLabel', 'Merge into')}
                  </Label>
                  <div className="flex items-center gap-2 px-3 py-2 bg-muted rounded-md">
                    <span className="font-medium">{targetProject.name}</span>
                    <span className="text-muted-foreground text-sm">
                      ({targetProject.linked_nodes_count}{' '}
                      {targetProject.linked_nodes_count === 1
                        ? 'node'
                        : 'nodes'}
                      )
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
              {t('settings.swarm.merge.confirm', 'Merge Projects')}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
