import { useState, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { Plus, FolderGit2, Loader2, GitMerge, RefreshCw } from 'lucide-react';
import { Button } from '@/components/ui/button';
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from '@/components/ui/card';
import { TooltipProvider } from '@/components/ui/tooltip';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { SwarmProjectRow } from './SwarmProjectRow';
import { SwarmProjectDialog } from './SwarmProjectDialog';
import { MergeProjectsDialog } from './MergeProjectsDialog';
import {
  useSwarmProjects,
  useSwarmProjectNodes,
  useSwarmProjectMutations,
} from '@/hooks/useSwarmProjects';
import type { SwarmProject, SwarmProjectWithNodes } from '@/types/swarm';

interface SwarmProjectsSectionProps {
  organizationId: string;
}

export function SwarmProjectsSection({
  organizationId,
}: SwarmProjectsSectionProps) {
  const { t } = useTranslation(['settings', 'common']);

  // State for dialogs
  const [isCreateDialogOpen, setIsCreateDialogOpen] = useState(false);
  const [editingProject, setEditingProject] = useState<SwarmProject | null>(
    null
  );
  const [mergingProject, setMergingProject] =
    useState<SwarmProjectWithNodes | null>(null);
  const [expandedProjectId, setExpandedProjectId] = useState<string | null>(
    null
  );

  // Fetch projects
  const {
    data: projects = [],
    isLoading,
    error,
    refetch,
  } = useSwarmProjects({
    organizationId,
    enabled: !!organizationId,
  });

  // Fetch nodes for expanded project
  const { data: expandedNodes = [], isLoading: isLoadingNodes } =
    useSwarmProjectNodes({
      projectId: expandedProjectId || '',
      enabled: !!expandedProjectId,
    });

  // Mutations
  const mutations = useSwarmProjectMutations({
    organizationId,
    onCreateSuccess: () => {
      setIsCreateDialogOpen(false);
    },
    onUpdateSuccess: () => {
      setEditingProject(null);
    },
    onDeleteSuccess: () => {
      // Clear expanded state if we deleted the expanded project
    },
    onMergeSuccess: () => {
      setMergingProject(null);
    },
  });

  // Handlers
  const handleToggleExpand = useCallback((projectId: string) => {
    setExpandedProjectId((prev) => (prev === projectId ? null : projectId));
  }, []);

  const handleCreate = async (data: {
    name: string;
    description: string | null;
  }) => {
    await mutations.createProject.mutateAsync({
      organization_id: organizationId,
      name: data.name,
      description: data.description,
    });
  };

  const handleEdit = async (data: {
    name: string;
    description: string | null;
  }) => {
    if (!editingProject) return;
    await mutations.updateProject.mutateAsync({
      projectId: editingProject.id,
      data: {
        name: data.name,
        description: data.description,
      },
    });
  };

  const handleDelete = async (project: SwarmProjectWithNodes) => {
    const confirmed = window.confirm(
      t(
        'settings.swarm.projects.deleteConfirm.description',
        'This will delete "{{name}}" and unlink all {{count}} nodes. This action cannot be undone.',
        {
          name: project.name,
          count: project.linked_nodes_count,
        }
      )
    );
    if (!confirmed) return;

    await mutations.deleteProject.mutateAsync(project.id);
    if (expandedProjectId === project.id) {
      setExpandedProjectId(null);
    }
  };

  const handleMerge = async (sourceId: string) => {
    if (!mergingProject) return;
    await mutations.mergeProjects.mutateAsync({
      targetId: mergingProject.id,
      sourceId,
    });
  };

  const handleUnlinkNode = async (projectId: string, nodeId: string) => {
    await mutations.unlinkNode.mutateAsync({ projectId, nodeId });
  };

  // Render empty state
  if (!organizationId) {
    return (
      <Card>
        <CardContent className="py-8">
          <Alert>
            <AlertDescription>
              {t(
                'settings.swarm.projects.noOrganization',
                'Please select an organization to manage swarm projects.'
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
              <FolderGit2 className="h-5 w-5 text-muted-foreground" />
              <CardTitle className="text-lg">
                {t('settings.swarm.projects.title', 'Swarm Projects')}
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
                  {t('settings.swarm.projects.refresh', 'Refresh')}
                </span>
              </Button>
              <Button
                size="sm"
                onClick={() => setIsCreateDialogOpen(true)}
                className="h-8"
              >
                <Plus className="h-4 w-4 mr-1" />
                <span className="hidden sm:inline">
                  {t('settings.swarm.projects.add', 'Add Project')}
                </span>
                <span className="sm:hidden">
                  {t('settings.swarm.projects.addShort', 'Add')}
                </span>
              </Button>
            </div>
          </div>
          <CardDescription>
            {t(
              'settings.swarm.projects.description',
              'Manage shared projects across your swarm nodes. Link local projects to share tasks between nodes.'
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
                    'settings.swarm.projects.error',
                    'Failed to load swarm projects. Please try again.'
                  )}
                </AlertDescription>
              </Alert>
            </div>
          ) : projects.length === 0 ? (
            <div className="text-center py-8 px-4">
              <FolderGit2 className="h-12 w-12 mx-auto text-muted-foreground/50 mb-4" />
              <p className="text-muted-foreground mb-4">
                {t(
                  'settings.swarm.projects.empty',
                  'No swarm projects yet. Create one to start sharing projects across nodes.'
                )}
              </p>
              <Button
                variant="outline"
                onClick={() => setIsCreateDialogOpen(true)}
              >
                <Plus className="h-4 w-4 mr-2" />
                {t(
                  'settings.swarm.projects.createFirst',
                  'Create your first project'
                )}
              </Button>
            </div>
          ) : (
            <div className="border-t border-border">
              {[...projects]
                .sort((a, b) => a.name.localeCompare(b.name))
                .map((project) => (
                  <SwarmProjectRow
                    key={project.id}
                    project={project}
                    nodes={
                      expandedProjectId === project.id ? expandedNodes : []
                    }
                    isLoadingNodes={
                      expandedProjectId === project.id && isLoadingNodes
                    }
                    isExpanded={expandedProjectId === project.id}
                    onToggleExpand={() => handleToggleExpand(project.id)}
                    onEdit={() =>
                      setEditingProject({
                        ...project,
                        metadata: project.metadata,
                      })
                    }
                    onDelete={() => handleDelete(project)}
                    onUnlinkNode={(nodeId) =>
                      handleUnlinkNode(project.id, nodeId)
                    }
                  />
                ))}

              {/* Merge button at bottom if multiple projects */}
              {projects.length > 1 && (
                <div className="px-4 py-3 border-t border-border">
                  <Button
                    variant="outline"
                    size="sm"
                    className="w-full sm:w-auto"
                    onClick={() => setMergingProject(projects[0])}
                  >
                    <GitMerge className="h-4 w-4 mr-2" />
                    {t(
                      'settings.swarm.projects.mergeProjects',
                      'Merge Projects'
                    )}
                  </Button>
                </div>
              )}
            </div>
          )}
        </CardContent>
      </Card>

      {/* Create Dialog */}
      <SwarmProjectDialog
        open={isCreateDialogOpen}
        onOpenChange={setIsCreateDialogOpen}
        onSave={handleCreate}
        isSaving={mutations.createProject.isPending}
      />

      {/* Edit Dialog */}
      <SwarmProjectDialog
        open={!!editingProject}
        onOpenChange={(open: boolean) => !open && setEditingProject(null)}
        project={editingProject}
        onSave={handleEdit}
        isSaving={mutations.updateProject.isPending}
      />

      {/* Merge Dialog */}
      {mergingProject && (
        <MergeProjectsDialog
          open={!!mergingProject}
          onOpenChange={(open: boolean) => !open && setMergingProject(null)}
          projects={projects}
          targetProject={mergingProject}
          onMerge={handleMerge}
          isMerging={mutations.mergeProjects.isPending}
        />
      )}
    </TooltipProvider>
  );
}
