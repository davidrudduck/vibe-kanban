import { useState, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import {
  Plus,
  FileText,
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
import { SwarmTemplateDialog } from './SwarmTemplateDialog';
import { MergeTemplatesDialog } from './MergeTemplatesDialog';
import {
  useSwarmTemplates,
  useSwarmTemplateMutations,
} from '@/hooks/useSwarmTemplates';
import type { SwarmTemplate } from '@/types/swarm';

interface SwarmTemplatesSectionProps {
  organizationId: string;
}

export function SwarmTemplatesSection({
  organizationId,
}: SwarmTemplatesSectionProps) {
  const { t } = useTranslation(['settings', 'common']);

  // State for dialogs
  const [isCreateDialogOpen, setIsCreateDialogOpen] = useState(false);
  const [editingTemplate, setEditingTemplate] = useState<SwarmTemplate | null>(
    null
  );
  const [mergingTemplate, setMergingTemplate] = useState<SwarmTemplate | null>(
    null
  );

  // Fetch templates
  const {
    data: templates = [],
    isLoading,
    error,
    refetch,
  } = useSwarmTemplates({
    organizationId,
    enabled: !!organizationId,
  });

  // Mutations
  const mutations = useSwarmTemplateMutations({
    organizationId,
    onCreateSuccess: () => {
      setIsCreateDialogOpen(false);
    },
    onUpdateSuccess: () => {
      setEditingTemplate(null);
    },
    onMergeSuccess: () => {
      setMergingTemplate(null);
    },
  });

  // Handlers
  const handleCreate = async (data: { name: string; content: string }) => {
    await mutations.createTemplate.mutateAsync({
      organization_id: organizationId,
      name: data.name,
      content: data.content,
    });
  };

  const handleEdit = async (data: { name: string; content: string }) => {
    if (!editingTemplate) return;
    await mutations.updateTemplate.mutateAsync({
      templateId: editingTemplate.id,
      data: {
        name: data.name,
        content: data.content,
      },
    });
  };

  const handleDelete = useCallback(
    async (template: SwarmTemplate) => {
      const confirmed = window.confirm(
        t(
          'settings.swarm.templates.deleteConfirm',
          'Are you sure you want to delete the template "{{name}}"? This action cannot be undone.',
          { name: template.name }
        )
      );
      if (!confirmed) return;

      await mutations.deleteTemplate.mutateAsync(template.id);
    },
    [mutations.deleteTemplate, t]
  );

  const handleMerge = async (sourceId: string) => {
    if (!mergingTemplate) return;
    await mutations.mergeTemplates.mutateAsync({
      targetId: mergingTemplate.id,
      sourceId,
    });
  };

  // Helper to truncate content for display
  const truncateContent = (content: string, maxLength = 100) => {
    if (content.length <= maxLength) return content;
    return content.substring(0, maxLength).trim() + '...';
  };

  // Render empty state
  if (!organizationId) {
    return (
      <Card>
        <CardContent className="py-8">
          <Alert>
            <AlertDescription>
              {t(
                'settings.swarm.templates.noOrganization',
                'Please select an organization to manage swarm templates.'
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
              <FileText className="h-5 w-5 text-muted-foreground" />
              <CardTitle className="text-lg">
                {t('settings.swarm.templates.title', 'Swarm Templates')}
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
                  {t('settings.swarm.templates.refresh', 'Refresh')}
                </span>
              </Button>
              <Button
                size="sm"
                onClick={() => setIsCreateDialogOpen(true)}
                className="h-8"
              >
                <Plus className="h-4 w-4 mr-1" />
                <span className="hidden sm:inline">
                  {t('settings.swarm.templates.add', 'Add Template')}
                </span>
                <span className="sm:hidden">
                  {t('settings.swarm.templates.addShort', 'Add')}
                </span>
              </Button>
            </div>
          </div>
          <CardDescription>
            {t(
              'settings.swarm.templates.description',
              'Manage organization-wide templates for task descriptions using @mentions.'
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
                    'settings.swarm.templates.error',
                    'Failed to load swarm templates. Please try again.'
                  )}
                </AlertDescription>
              </Alert>
            </div>
          ) : templates.length === 0 ? (
            <div className="text-center py-8 px-4">
              <FileText className="h-12 w-12 mx-auto text-muted-foreground/50 mb-4" />
              <p className="text-muted-foreground mb-4">
                {t(
                  'settings.swarm.templates.empty',
                  'No swarm templates yet. Create templates to reuse content across task descriptions.'
                )}
              </p>
              <Button
                variant="outline"
                onClick={() => setIsCreateDialogOpen(true)}
              >
                <Plus className="h-4 w-4 mr-2" />
                {t(
                  'settings.swarm.templates.createFirst',
                  'Create your first template'
                )}
              </Button>
            </div>
          ) : (
            <div className="border-t border-border">
              {/* Templates List */}
              <div className="divide-y divide-border">
                {templates.map((template) => (
                  <div
                    key={template.id}
                    className="flex items-start justify-between px-4 py-3 sm:px-6 hover:bg-muted/50 transition-colors gap-4"
                  >
                    <div className="min-w-0 flex-1">
                      <div className="flex items-center gap-2 mb-1">
                        <span className="font-medium text-sm">
                          @{template.name}
                        </span>
                      </div>
                      <p className="text-sm text-muted-foreground line-clamp-2">
                        {truncateContent(template.content)}
                      </p>
                    </div>
                    <div className="flex items-center gap-1 shrink-0">
                      <Button
                        variant="ghost"
                        size="sm"
                        className="h-8 w-8 p-0"
                        onClick={() => setEditingTemplate(template)}
                      >
                        <Pencil className="h-4 w-4" />
                        <span className="sr-only">
                          {t('settings.swarm.templates.edit', 'Edit')}
                        </span>
                      </Button>
                      <Button
                        variant="ghost"
                        size="sm"
                        className="h-8 w-8 p-0 text-destructive hover:text-destructive"
                        onClick={() => handleDelete(template)}
                        disabled={mutations.deleteTemplate.isPending}
                      >
                        <Trash2 className="h-4 w-4" />
                        <span className="sr-only">
                          {t('settings.swarm.templates.delete', 'Delete')}
                        </span>
                      </Button>
                    </div>
                  </div>
                ))}
              </div>

              {/* Merge button at bottom if multiple templates */}
              {templates.length > 1 && (
                <div className="px-4 py-3 border-t border-border">
                  <Button
                    variant="outline"
                    size="sm"
                    className="w-full sm:w-auto"
                    onClick={() => setMergingTemplate(templates[0])}
                  >
                    <GitMerge className="h-4 w-4 mr-2" />
                    {t(
                      'settings.swarm.templates.mergeTemplates',
                      'Merge Templates'
                    )}
                  </Button>
                </div>
              )}
            </div>
          )}
        </CardContent>
      </Card>

      {/* Create Dialog */}
      <SwarmTemplateDialog
        open={isCreateDialogOpen}
        onOpenChange={setIsCreateDialogOpen}
        onSave={handleCreate}
        isSaving={mutations.createTemplate.isPending}
      />

      {/* Edit Dialog */}
      <SwarmTemplateDialog
        open={!!editingTemplate}
        onOpenChange={(open: boolean) => !open && setEditingTemplate(null)}
        template={editingTemplate}
        onSave={handleEdit}
        isSaving={mutations.updateTemplate.isPending}
      />

      {/* Merge Dialog */}
      {mergingTemplate && (
        <MergeTemplatesDialog
          open={!!mergingTemplate}
          onOpenChange={(open: boolean) => !open && setMergingTemplate(null)}
          templates={templates}
          targetTemplate={mergingTemplate}
          onMerge={handleMerge}
          isMerging={mutations.mergeTemplates.isPending}
        />
      )}
    </TooltipProvider>
  );
}
