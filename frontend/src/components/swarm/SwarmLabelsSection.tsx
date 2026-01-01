import { useState, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import {
  Plus,
  Tags,
  Loader2,
  GitMerge,
  RefreshCw,
  Pencil,
  Trash2,
} from 'lucide-react';
import { Button } from '@/components/ui/button';
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from '@/components/ui/card';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { TooltipProvider } from '@/components/ui/tooltip';
import { SwarmLabelDialog } from './SwarmLabelDialog';
import { MergeLabelsDialog } from './MergeLabelsDialog';
import { useSwarmLabels, useSwarmLabelMutations } from '@/hooks/useSwarmLabels';
import type { SwarmLabel } from '@/types/swarm';
import type { Label } from 'shared/types';
import { LabelBadge } from '@/components/labels/LabelBadge';

// Helper to convert SwarmLabel to local Label format for LabelBadge
function swarmLabelToLabel(swarmLabel: SwarmLabel): Label {
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

interface SwarmLabelsSectionProps {
  organizationId: string;
}

export function SwarmLabelsSection({
  organizationId,
}: SwarmLabelsSectionProps) {
  const { t } = useTranslation(['settings', 'common']);

  // State for dialogs
  const [isCreateDialogOpen, setIsCreateDialogOpen] = useState(false);
  const [editingLabel, setEditingLabel] = useState<SwarmLabel | null>(null);
  const [mergingLabel, setMergingLabel] = useState<SwarmLabel | null>(null);

  // Fetch labels
  const {
    data: labels = [],
    isLoading,
    error,
    refetch,
  } = useSwarmLabels({
    organizationId,
    enabled: !!organizationId,
  });

  // Mutations
  const mutations = useSwarmLabelMutations({
    organizationId,
    onCreateSuccess: () => {
      setIsCreateDialogOpen(false);
    },
    onUpdateSuccess: () => {
      setEditingLabel(null);
    },
    onMergeSuccess: () => {
      setMergingLabel(null);
    },
  });

  // Handlers
  const handleCreate = async (data: {
    name: string;
    color: string;
    icon: string | null;
  }) => {
    await mutations.createLabel.mutateAsync({
      organization_id: organizationId,
      name: data.name,
      color: data.color,
      icon: data.icon,
    });
  };

  const handleEdit = async (data: {
    name: string;
    color: string;
    icon: string | null;
  }) => {
    if (!editingLabel) return;
    await mutations.updateLabel.mutateAsync({
      labelId: editingLabel.id,
      data: {
        name: data.name,
        color: data.color,
        icon: data.icon,
      },
    });
  };

  const handleDelete = useCallback(
    async (label: SwarmLabel) => {
      const confirmed = window.confirm(
        t(
          'settings.swarm.labels.deleteConfirm',
          'Are you sure you want to delete the label "{{name}}"? This action cannot be undone.',
          { name: label.name }
        )
      );
      if (!confirmed) return;

      await mutations.deleteLabel.mutateAsync(label.id);
    },
    [mutations.deleteLabel, t]
  );

  const handleMerge = async (sourceId: string) => {
    if (!mergingLabel) return;
    await mutations.mergeLabels.mutateAsync({
      targetId: mergingLabel.id,
      sourceId,
    });
  };

  // Render empty state
  if (!organizationId) {
    return (
      <Card>
        <CardContent className="py-8">
          <Alert>
            <AlertDescription>
              {t(
                'settings.swarm.labels.noOrganization',
                'Please select an organization to manage swarm labels.'
              )}
            </AlertDescription>
          </Alert>
        </CardContent>
      </Card>
    );
  }

  return (
    <TooltipProvider>
      <Card>
        <CardHeader className="space-y-1">
          <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
            <div className="flex items-center gap-2">
              <Tags className="h-5 w-5 text-muted-foreground" />
              <CardTitle className="text-lg">
                {t('settings.swarm.labels.title', 'Swarm Labels')}
              </CardTitle>
            </div>
            <div className="flex items-center gap-2">
              <Button
                variant="ghost"
                size="sm"
                onClick={() => refetch()}
                disabled={isLoading}
                className="h-8"
              >
                <RefreshCw
                  className={`h-4 w-4 ${isLoading ? 'animate-spin' : ''}`}
                />
                <span className="sr-only">
                  {t('settings.swarm.labels.refresh', 'Refresh')}
                </span>
              </Button>
              <Button
                size="sm"
                onClick={() => setIsCreateDialogOpen(true)}
                className="h-8"
              >
                <Plus className="h-4 w-4 mr-1" />
                <span className="hidden sm:inline">
                  {t('settings.swarm.labels.add', 'Add Label')}
                </span>
                <span className="sm:hidden">
                  {t('settings.swarm.labels.addShort', 'Add')}
                </span>
              </Button>
            </div>
          </div>
          <CardDescription>
            {t(
              'settings.swarm.labels.description',
              'Manage organization-wide labels that can be used across all swarm projects.'
            )}
          </CardDescription>
        </CardHeader>

        <CardContent className="px-0 pb-0 sm:px-0">
          {isLoading ? (
            <div className="flex items-center justify-center py-8">
              <Loader2 className="h-6 w-6 animate-spin text-muted-foreground" />
            </div>
          ) : error ? (
            <div className="px-4 pb-4 sm:px-6">
              <Alert variant="destructive">
                <AlertDescription>
                  {t(
                    'settings.swarm.labels.error',
                    'Failed to load swarm labels. Please try again.'
                  )}
                </AlertDescription>
              </Alert>
            </div>
          ) : labels.length === 0 ? (
            <div className="text-center py-8 px-4">
              <Tags className="h-12 w-12 mx-auto text-muted-foreground/50 mb-4" />
              <p className="text-muted-foreground mb-4">
                {t(
                  'settings.swarm.labels.empty',
                  'No swarm labels yet. Create labels to categorize tasks across your organization.'
                )}
              </p>
              <Button
                variant="outline"
                onClick={() => setIsCreateDialogOpen(true)}
              >
                <Plus className="h-4 w-4 mr-2" />
                {t(
                  'settings.swarm.labels.createFirst',
                  'Create your first label'
                )}
              </Button>
            </div>
          ) : (
            <div className="border-t border-border">
              {/* Labels List */}
              <div className="divide-y divide-border">
                {labels.map((label) => (
                  <div
                    key={label.id}
                    className="flex items-center justify-between px-4 py-3 sm:px-6 hover:bg-muted/50 transition-colors"
                  >
                    <div className="flex items-center gap-3 min-w-0">
                      <LabelBadge label={swarmLabelToLabel(label)} size="md" />
                      {label.project_id && (
                        <span className="text-xs text-muted-foreground">
                          {t(
                            'settings.swarm.labels.projectSpecific',
                            'Project-specific'
                          )}
                        </span>
                      )}
                    </div>
                    <div className="flex items-center gap-1">
                      <Button
                        variant="ghost"
                        size="sm"
                        className="h-8 w-8 p-0"
                        onClick={() => setEditingLabel(label)}
                      >
                        <Pencil className="h-4 w-4" />
                        <span className="sr-only">
                          {t('settings.swarm.labels.edit', 'Edit')}
                        </span>
                      </Button>
                      <Button
                        variant="ghost"
                        size="sm"
                        className="h-8 w-8 p-0 text-destructive hover:text-destructive"
                        onClick={() => handleDelete(label)}
                        disabled={mutations.deleteLabel.isPending}
                      >
                        <Trash2 className="h-4 w-4" />
                        <span className="sr-only">
                          {t('settings.swarm.labels.delete', 'Delete')}
                        </span>
                      </Button>
                    </div>
                  </div>
                ))}
              </div>

              {/* Merge button at bottom if multiple labels */}
              {labels.length > 1 && (
                <div className="px-4 py-3 border-t border-border">
                  <Button
                    variant="outline"
                    size="sm"
                    className="w-full sm:w-auto"
                    onClick={() => setMergingLabel(labels[0])}
                  >
                    <GitMerge className="h-4 w-4 mr-2" />
                    {t('settings.swarm.labels.mergeLabels', 'Merge Labels')}
                  </Button>
                </div>
              )}
            </div>
          )}
        </CardContent>
      </Card>

      {/* Create Dialog */}
      <SwarmLabelDialog
        open={isCreateDialogOpen}
        onOpenChange={setIsCreateDialogOpen}
        onSave={handleCreate}
        isSaving={mutations.createLabel.isPending}
      />

      {/* Edit Dialog */}
      <SwarmLabelDialog
        open={!!editingLabel}
        onOpenChange={(open: boolean) => !open && setEditingLabel(null)}
        label={editingLabel}
        onSave={handleEdit}
        isSaving={mutations.updateLabel.isPending}
      />

      {/* Merge Dialog */}
      {mergingLabel && (
        <MergeLabelsDialog
          open={!!mergingLabel}
          onOpenChange={(open: boolean) => !open && setMergingLabel(null)}
          labels={labels}
          targetLabel={mergingLabel}
          onMerge={handleMerge}
          isMerging={mutations.mergeLabels.isPending}
        />
      )}
    </TooltipProvider>
  );
}
