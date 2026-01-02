import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useQuery } from '@tanstack/react-query';
import {
  FileText,
  Loader2,
  RefreshCw,
  ArrowUpToLine,
  Check,
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
import { Badge } from '@/components/ui/badge';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { templatesApi } from '@/lib/api';
import {
  useSwarmTemplates,
  useSwarmTemplateMutations,
} from '@/hooks/useSwarmTemplates';
import type { Template } from 'shared/types';

interface NodeTemplatesSectionProps {
  organizationId: string;
}

export function NodeTemplatesSection({
  organizationId,
}: NodeTemplatesSectionProps) {
  const { t } = useTranslation(['settings', 'common']);

  // State for promote dialog
  const [promotingTemplate, setPromotingTemplate] = useState<Template | null>(
    null
  );

  // Fetch local templates
  const {
    data: localTemplates = [],
    isLoading,
    error,
    refetch,
  } = useQuery({
    queryKey: ['templates'],
    queryFn: () => templatesApi.list(),
    staleTime: 30_000,
  });

  // Fetch swarm templates to check which ones are already promoted
  const { data: swarmTemplates = [] } = useSwarmTemplates({
    organizationId,
    enabled: !!organizationId,
  });

  // Mutations for creating swarm template
  const mutations = useSwarmTemplateMutations({
    organizationId,
    onCreateSuccess: () => {
      setPromotingTemplate(null);
    },
  });

  // Check if a local template has a matching swarm template (by name)
  const isAlreadyPromoted = (template: Template): boolean => {
    return swarmTemplates.some(
      (st) => st.name.toLowerCase() === template.template_name.toLowerCase()
    );
  };

  // Handle promote action
  const handlePromote = async () => {
    if (!promotingTemplate) return;

    await mutations.createTemplate.mutateAsync({
      organization_id: organizationId,
      name: promotingTemplate.template_name,
      content: promotingTemplate.content,
    });
  };

  // Helper to truncate content for display
  const truncateContent = (content: string, maxLength = 100) => {
    if (content.length <= maxLength) return content;
    return content.substring(0, maxLength).trim() + '...';
  };

  if (!organizationId) {
    return (
      <Card>
        <CardContent className="py-8">
          <Alert>
            <AlertDescription>
              {t(
                'settings.swarm.nodeTemplates.noOrganization',
                'Please select an organization to view local templates.'
              )}
            </AlertDescription>
          </Alert>
        </CardContent>
      </Card>
    );
  }

  return (
    <>
      <Card>
        <CardHeader className="space-y-1">
          <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
            <div className="flex items-center gap-2">
              <FileText className="h-5 w-5 text-muted-foreground" />
              <CardTitle className="text-lg">
                {t('settings.swarm.nodeTemplates.title', 'Local Templates')}
              </CardTitle>
            </div>
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
                {t('settings.swarm.nodeTemplates.refresh', 'Refresh')}
              </span>
            </Button>
          </div>
          <CardDescription>
            {t(
              'settings.swarm.nodeTemplates.description',
              'Promote local templates to make them available across all nodes in the organization.'
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
                    'settings.swarm.nodeTemplates.error',
                    'Failed to load local templates. Please try again.'
                  )}
                </AlertDescription>
              </Alert>
            </div>
          ) : localTemplates.length === 0 ? (
            <div className="text-center py-8 px-4">
              <FileText className="h-12 w-12 mx-auto text-muted-foreground/50 mb-4" />
              <p className="text-muted-foreground">
                {t(
                  'settings.swarm.nodeTemplates.noTemplates',
                  'No local templates on this node. Create templates in Task settings to use @mentions.'
                )}
              </p>
            </div>
          ) : (
            <div className="border-t border-border divide-y divide-border">
              {localTemplates.map((template) => {
                const promoted = isAlreadyPromoted(template);

                return (
                  <div
                    key={template.id}
                    className="flex items-start justify-between px-4 py-3 sm:px-6 hover:bg-muted/50 transition-colors gap-4"
                  >
                    <div className="min-w-0 flex-1">
                      <div className="flex items-center gap-2 mb-1">
                        <span className="font-medium text-sm">
                          @{template.template_name}
                        </span>
                        {promoted && (
                          <Badge variant="secondary" className="text-xs gap-1">
                            <Check className="h-3 w-3" />
                            {t(
                              'settings.swarm.nodeTemplates.promoted',
                              'In Swarm'
                            )}
                          </Badge>
                        )}
                      </div>
                      <p className="text-sm text-muted-foreground line-clamp-2">
                        {truncateContent(template.content)}
                      </p>
                    </div>
                    {!promoted && (
                      <Button
                        variant="outline"
                        size="sm"
                        className="shrink-0"
                        onClick={() => setPromotingTemplate(template)}
                      >
                        <ArrowUpToLine className="h-4 w-4 mr-1" />
                        <span className="hidden sm:inline">
                          {t(
                            'settings.swarm.nodeTemplates.promote',
                            'Promote to Swarm'
                          )}
                        </span>
                        <span className="sm:hidden">
                          {t(
                            'settings.swarm.nodeTemplates.promoteShort',
                            'Promote'
                          )}
                        </span>
                      </Button>
                    )}
                  </div>
                );
              })}
            </div>
          )}
        </CardContent>
      </Card>

      {/* Promote to Swarm Dialog */}
      <Dialog
        open={!!promotingTemplate}
        onOpenChange={(open) => {
          if (!open) {
            setPromotingTemplate(null);
          }
        }}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>
              {t(
                'settings.swarm.nodeTemplates.promoteDialog.title',
                'Promote to Swarm Template'
              )}
            </DialogTitle>
            <DialogDescription>
              {t(
                'settings.swarm.nodeTemplates.promoteDialog.description',
                'This will create a swarm template that can be used across all nodes in the organization.'
              )}
            </DialogDescription>
          </DialogHeader>

          {promotingTemplate && (
            <div className="space-y-4 py-4">
              <div className="p-4 bg-muted rounded-md">
                <div className="flex items-center gap-2 mb-2">
                  <FileText className="h-4 w-4 text-muted-foreground" />
                  <span className="font-medium">
                    @{promotingTemplate.template_name}
                  </span>
                </div>
                <p className="text-sm text-muted-foreground">
                  {truncateContent(promotingTemplate.content, 200)}
                </p>
              </div>
              <p className="text-sm text-muted-foreground">
                {t(
                  'settings.swarm.nodeTemplates.promoteDialog.note',
                  'The template will be copied to the swarm. The local template will remain unchanged.'
                )}
              </p>
            </div>
          )}

          <DialogFooter>
            <Button
              variant="outline"
              onClick={() => setPromotingTemplate(null)}
            >
              {t('common:cancel', 'Cancel')}
            </Button>
            <Button
              onClick={handlePromote}
              disabled={mutations.createTemplate.isPending}
            >
              {mutations.createTemplate.isPending ? (
                <Loader2 className="h-4 w-4 mr-2 animate-spin" />
              ) : (
                <ArrowUpToLine className="h-4 w-4 mr-2" />
              )}
              {t(
                'settings.swarm.nodeTemplates.promoteDialog.confirm',
                'Promote Template'
              )}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}
