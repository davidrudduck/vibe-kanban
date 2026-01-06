import { useState, useEffect, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { useQuery } from '@tanstack/react-query';
import {
  Monitor,
  Loader2,
  RefreshCw,
  Link2,
  Unlink,
  FolderGit2,
  ChevronDown,
  ChevronRight,
  Plus,
  AlertCircle,
} from 'lucide-react';
import { formatDistanceToNow, differenceInMinutes } from 'date-fns';
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
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from '@/components/ui/tooltip';
import { nodesApi } from '@/lib/api';
import {
  useSwarmProjects,
  useSwarmProjectMutations,
} from '@/hooks/useSwarmProjects';
import type { NodeProject } from '@/types/nodes';

/** Threshold in minutes after which a project is considered stale */
const STALE_THRESHOLD_MINUTES = 10;

/** Check if a project is stale based on last_seen_at */
function isProjectStale(lastSeenAt: string): boolean {
  const lastSeen = new Date(lastSeenAt);
  const minutesAgo = differenceInMinutes(new Date(), lastSeen);
  return minutesAgo > STALE_THRESHOLD_MINUTES;
}

interface NodeProjectsSectionProps {
  organizationId: string;
}

export function NodeProjectsSection({
  organizationId,
}: NodeProjectsSectionProps) {
  const { t } = useTranslation(['settings', 'common']);

  // State for expanded nodes and linking dialog
  const [expandedNodeIds, setExpandedNodeIds] = useState<Set<string>>(
    new Set()
  );
  const [linkingProject, setLinkingProject] = useState<{
    nodeId: string;
    project: NodeProject;
  } | null>(null);
  const [selectedSwarmProjectId, setSelectedSwarmProjectId] = useState<
    string | null
  >(null);
  const [nodeProjects, setNodeProjects] = useState<
    Record<string, NodeProject[]>
  >({});
  const [loadingNodeProjects, setLoadingNodeProjects] = useState<Set<string>>(
    new Set()
  );

  // State for dialog tabs and new project form
  const [dialogTab, setDialogTab] = useState<'existing' | 'create'>('existing');
  const [newProjectName, setNewProjectName] = useState('');
  const [newProjectDescription, setNewProjectDescription] = useState('');

  // Fetch nodes
  const {
    data: nodes = [],
    isLoading: isLoadingNodes,
    error: nodesError,
    refetch: refetchNodes,
  } = useQuery({
    queryKey: ['nodes', organizationId],
    queryFn: () => nodesApi.list(organizationId),
    enabled: !!organizationId,
    staleTime: 30_000,
  });

  // Fetch swarm projects for linking
  const { data: swarmProjects = [] } = useSwarmProjects({
    organizationId,
    enabled: !!organizationId,
  });

  // Mutations for linking and creating
  const mutations = useSwarmProjectMutations({
    organizationId,
    onLinkNodeSuccess: () => {
      setLinkingProject(null);
      setSelectedSwarmProjectId(null);
      setNewProjectName('');
      setNewProjectDescription('');
      setDialogTab('existing');
      // Refetch to update linked status
      refetchNodes();
    },
  });

  // Fetch projects for expanded nodes
  const fetchNodeProjects = useCallback(async (nodeId: string) => {
    setLoadingNodeProjects((prev) => new Set(prev).add(nodeId));
    try {
      const projects = await nodesApi.listProjects(nodeId);
      setNodeProjects((prev) => ({ ...prev, [nodeId]: projects }));
    } catch (err) {
      console.error('Failed to fetch node projects:', err);
    } finally {
      setLoadingNodeProjects((prev) => {
        const next = new Set(prev);
        next.delete(nodeId);
        return next;
      });
    }
  }, []);

  // Fetch projects when node is expanded
  useEffect(() => {
    expandedNodeIds.forEach((nodeId) => {
      if (!nodeProjects[nodeId] && !loadingNodeProjects.has(nodeId)) {
        fetchNodeProjects(nodeId);
      }
    });
  }, [expandedNodeIds, nodeProjects, loadingNodeProjects, fetchNodeProjects]);

  // Toggle node expansion
  const handleToggleNode = (nodeId: string) => {
    setExpandedNodeIds((prev) => {
      const next = new Set(prev);
      if (next.has(nodeId)) {
        next.delete(nodeId);
      } else {
        next.add(nodeId);
      }
      return next;
    });
  };

  // Handle link to existing project action
  const handleLink = async () => {
    if (!linkingProject || !selectedSwarmProjectId) return;

    await mutations.linkNode.mutateAsync({
      projectId: selectedSwarmProjectId,
      data: {
        node_id: linkingProject.nodeId,
        local_project_id: linkingProject.project.local_project_id,
        git_repo_path: linkingProject.project.git_repo_path,
      },
    });
  };

  // Handle create new project and link action
  const handleCreateAndLink = async () => {
    if (!linkingProject || !newProjectName.trim()) return;

    // First create the project
    const project = await mutations.createProject.mutateAsync({
      organization_id: organizationId,
      name: newProjectName.trim(),
      description: newProjectDescription.trim() || null,
    });

    // Then link the node project to it
    await mutations.linkNode.mutateAsync({
      projectId: project.id,
      data: {
        node_id: linkingProject.nodeId,
        local_project_id: linkingProject.project.local_project_id,
        git_repo_path: linkingProject.project.git_repo_path,
      },
    });
  };

  if (!organizationId) {
    return (
      <Card>
        <CardContent className="py-8">
          <Alert>
            <AlertDescription>
              {t(
                'settings.swarm.nodeProjects.noOrganization',
                'Please select an organization to view node projects.'
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
              <Monitor className="h-5 w-5 text-muted-foreground" />
              <CardTitle className="text-lg">
                {t('settings.swarm.nodeProjects.title', 'Node Projects')}
              </CardTitle>
            </div>
            <Button
              variant="ghost"
              size="sm"
              onClick={() => refetchNodes()}
              disabled={isLoadingNodes}
              className="h-8"
            >
              <RefreshCw
                className={`h-4 w-4 ${isLoadingNodes ? 'animate-spin' : ''}`}
              />
              <span className="sr-only">
                {t('settings.swarm.nodeProjects.refresh', 'Refresh')}
              </span>
            </Button>
          </div>
          <CardDescription>
            {t(
              'settings.swarm.nodeProjects.description',
              'View local projects on each node and link them to swarm projects for cross-node task sharing.'
            )}
          </CardDescription>
        </CardHeader>

        <CardContent className="px-0 pb-0 sm:px-0">
          {isLoadingNodes ? (
            <div className="flex items-center justify-center py-8">
              <Loader2 className="h-6 w-6 animate-spin text-muted-foreground" />
            </div>
          ) : nodesError ? (
            <div className="px-4 pb-4 sm:px-6">
              <Alert variant="destructive">
                <AlertDescription>
                  {t(
                    'settings.swarm.nodeProjects.error',
                    'Failed to load nodes. Please try again.'
                  )}
                </AlertDescription>
              </Alert>
            </div>
          ) : nodes.length === 0 ? (
            <div className="text-center py-8 px-4">
              <Monitor className="h-12 w-12 mx-auto text-muted-foreground/50 mb-4" />
              <p className="text-muted-foreground">
                {t(
                  'settings.swarm.nodeProjects.noNodes',
                  'No nodes connected to this organization yet.'
                )}
              </p>
            </div>
          ) : (
            <div className="border-t border-border divide-y divide-border">
              {nodes.map((node) => {
                const isExpanded = expandedNodeIds.has(node.id);
                const projects = nodeProjects[node.id] || [];
                const isLoadingProjects = loadingNodeProjects.has(node.id);

                return (
                  <div key={node.id}>
                    {/* Node Header */}
                    <button
                      className="w-full flex items-center justify-between px-4 py-3 sm:px-6 hover:bg-muted/50 transition-colors text-left"
                      onClick={() => handleToggleNode(node.id)}
                    >
                      <div className="flex items-center gap-3 min-w-0">
                        {isExpanded ? (
                          <ChevronDown className="h-4 w-4 text-muted-foreground shrink-0" />
                        ) : (
                          <ChevronRight className="h-4 w-4 text-muted-foreground shrink-0" />
                        )}
                        <Monitor className="h-4 w-4 text-muted-foreground shrink-0" />
                        <span className="font-medium truncate">
                          {node.name}
                        </span>
                        <Badge
                          variant={
                            node.status === 'online' ? 'default' : 'secondary'
                          }
                          className="shrink-0"
                        >
                          {node.status}
                        </Badge>
                      </div>
                      {isLoadingProjects && (
                        <Loader2 className="h-4 w-4 animate-spin text-muted-foreground" />
                      )}
                    </button>

                    {/* Node Projects */}
                    {isExpanded && (
                      <div className="bg-muted/30 border-t border-border">
                        {isLoadingProjects ? (
                          <div className="flex items-center justify-center py-4">
                            <Loader2 className="h-5 w-5 animate-spin text-muted-foreground" />
                          </div>
                        ) : projects.length === 0 ? (
                          <div className="px-4 py-4 sm:px-6 text-sm text-muted-foreground">
                            {t(
                              'settings.swarm.nodeProjects.noProjects',
                              'No projects registered on this node.'
                            )}
                          </div>
                        ) : (
                          <div className="divide-y divide-border">
                            {[...projects]
                              .sort((a, b) => a.name.localeCompare(b.name))
                              .map((project) => {
                                const shortId = project.local_project_id.slice(-4);
                                const isLinked = !!project.swarm_project_id;
                                const stale = isProjectStale(project.last_seen_at);
                                const lastSeenText = formatDistanceToNow(
                                  new Date(project.last_seen_at),
                                  { addSuffix: true }
                                );

                                return (
                                  <div
                                    key={project.id}
                                    className={`flex items-center justify-between px-4 py-3 sm:px-6 pl-12 sm:pl-14 ${
                                      stale ? 'opacity-60' : ''
                                    }`}
                                  >
                                    <div className="flex items-center gap-3 min-w-0">
                                      <FolderGit2 className="h-4 w-4 text-muted-foreground shrink-0" />
                                      <div className="min-w-0">
                                        <div className="flex items-center gap-2 text-sm">
                                          <span className="font-medium truncate">
                                            {project.name}
                                          </span>
                                          <span className="text-muted-foreground shrink-0">
                                            ({shortId})
                                          </span>
                                          {stale && (
                                            <Tooltip>
                                              <TooltipTrigger asChild>
                                                <Badge
                                                  variant="outline"
                                                  className="text-amber-600 border-amber-600/50 shrink-0"
                                                >
                                                  <AlertCircle className="h-3 w-3 mr-1" />
                                                  Stale
                                                </Badge>
                                              </TooltipTrigger>
                                              <TooltipContent>
                                                <p>
                                                  Last seen {lastSeenText}. Node may have
                                                  removed this project.
                                                </p>
                                              </TooltipContent>
                                            </Tooltip>
                                          )}
                                        </div>
                                        <p className="text-xs text-muted-foreground truncate">
                                          {project.git_repo_path}
                                        </p>
                                      </div>
                                    </div>
                                    {isLinked ? (
                                      <div className="flex items-center gap-2 shrink-0">
                                        <Badge variant="secondary" className="hidden sm:inline-flex">
                                          {project.swarm_project_name}
                                        </Badge>
                                        <Button
                                          variant="ghost"
                                          size="sm"
                                          className="text-destructive hover:text-destructive"
                                          onClick={(e) => {
                                            e.stopPropagation();
                                            if (project.swarm_project_id) {
                                              mutations.unlinkNode.mutate({
                                                projectId: project.swarm_project_id,
                                                nodeId: node.id,
                                              });
                                            }
                                          }}
                                        >
                                          <Unlink className="h-4 w-4 mr-1" />
                                          <span className="hidden sm:inline">
                                            {t(
                                              'settings.swarm.nodeProjects.unlinkFromSwarm',
                                              'Unlink'
                                            )}
                                          </span>
                                        </Button>
                                      </div>
                                    ) : (
                                      <Button
                                        variant="outline"
                                        size="sm"
                                        className="shrink-0"
                                        onClick={(e) => {
                                          e.stopPropagation();
                                          setLinkingProject({
                                            nodeId: node.id,
                                            project,
                                          });
                                        }}
                                      >
                                        <Link2 className="h-4 w-4 mr-1" />
                                        <span className="hidden sm:inline">
                                          {t(
                                            'settings.swarm.nodeProjects.linkToSwarm',
                                            'Link to Swarm'
                                          )}
                                        </span>
                                        <span className="sm:hidden">
                                          {t(
                                            'settings.swarm.nodeProjects.link',
                                            'Link'
                                          )}
                                        </span>
                                      </Button>
                                    )}
                                  </div>
                                );
                              })}
                          </div>
                        )}
                      </div>
                    )}
                  </div>
                );
              })}
            </div>
          )}
        </CardContent>
      </Card>

      {/* Link to Swarm Project Dialog */}
      <Dialog
        open={!!linkingProject}
        onOpenChange={(open) => {
          if (!open) {
            setLinkingProject(null);
            setSelectedSwarmProjectId(null);
            setNewProjectName('');
            setNewProjectDescription('');
            setDialogTab('existing');
          }
        }}
      >
        <DialogContent className="sm:max-w-[500px]">
          <DialogHeader>
            <DialogTitle>
              {t(
                'settings.swarm.nodeProjects.linkDialog.title',
                'Link to Swarm Project'
              )}
            </DialogTitle>
            <DialogDescription>
              {t(
                'settings.swarm.nodeProjects.linkDialog.description',
                'Select a swarm project to link this local project to. Tasks will be shared across all linked nodes.'
              )}
            </DialogDescription>
          </DialogHeader>

          {linkingProject && (
            <div className="space-y-4 py-4">
              {/* Local Project Display */}
              <div className="space-y-2">
                <Label>
                  {t(
                    'settings.swarm.nodeProjects.linkDialog.localProject',
                    'Local Project'
                  )}
                </Label>
                <div className="p-3 bg-muted rounded-md">
                  <p className="font-medium">
                    {linkingProject.project.git_repo_path.split('/').pop()}
                  </p>
                  <p className="text-sm text-muted-foreground">
                    {linkingProject.project.git_repo_path}
                  </p>
                </div>
              </div>

              {/* Tabs for Link Existing or Create New */}
              <Tabs
                value={dialogTab}
                onValueChange={(value) =>
                  setDialogTab(value as 'existing' | 'create')
                }
                className="w-full"
              >
                <TabsList className="grid w-full grid-cols-2">
                  <TabsTrigger value="existing">
                    <Link2 className="h-4 w-4 mr-2" />
                    {t(
                      'settings.swarm.nodeProjects.linkDialog.linkExisting',
                      'Link to Existing'
                    )}
                  </TabsTrigger>
                  <TabsTrigger value="create">
                    <Plus className="h-4 w-4 mr-2" />
                    {t(
                      'settings.swarm.nodeProjects.linkDialog.createNew',
                      'Create New'
                    )}
                  </TabsTrigger>
                </TabsList>

                {/* Link to Existing Tab */}
                <TabsContent value="existing" className="space-y-4 mt-4">
                  <div className="space-y-2">
                    <Label htmlFor="swarm-project-select">
                      {t(
                        'settings.swarm.nodeProjects.linkDialog.swarmProject',
                        'Swarm Project'
                      )}
                    </Label>
                    <Select
                      value={selectedSwarmProjectId || ''}
                      onValueChange={setSelectedSwarmProjectId}
                    >
                      <SelectTrigger id="swarm-project-select">
                        <SelectValue
                          placeholder={t(
                            'settings.swarm.nodeProjects.linkDialog.selectProject',
                            'Select a swarm project...'
                          )}
                        />
                      </SelectTrigger>
                      <SelectContent>
                        {swarmProjects.length === 0 ? (
                          <SelectItem value="no-projects" disabled>
                            {t(
                              'settings.swarm.nodeProjects.linkDialog.noProjects',
                              'No swarm projects available'
                            )}
                          </SelectItem>
                        ) : (
                          swarmProjects.map((sp) => (
                            <SelectItem key={sp.id} value={sp.id}>
                              {sp.name}
                            </SelectItem>
                          ))
                        )}
                      </SelectContent>
                    </Select>
                  </div>
                </TabsContent>

                {/* Create New Tab */}
                <TabsContent value="create" className="space-y-4 mt-4">
                  <div className="space-y-2">
                    <Label htmlFor="new-project-name">
                      {t(
                        'settings.swarm.nodeProjects.linkDialog.projectName',
                        'Project Name'
                      )}
                    </Label>
                    <Input
                      id="new-project-name"
                      value={newProjectName}
                      onChange={(e) => setNewProjectName(e.target.value)}
                      placeholder={t(
                        'settings.swarm.nodeProjects.linkDialog.projectNamePlaceholder',
                        'Enter project name...'
                      )}
                    />
                  </div>
                  <div className="space-y-2">
                    <Label htmlFor="new-project-description">
                      {t(
                        'settings.swarm.nodeProjects.linkDialog.projectDescription',
                        'Description'
                      )}{' '}
                      <span className="text-muted-foreground">
                        ({t('common:optional', 'Optional')})
                      </span>
                    </Label>
                    <Input
                      id="new-project-description"
                      value={newProjectDescription}
                      onChange={(e) => setNewProjectDescription(e.target.value)}
                      placeholder={t(
                        'settings.swarm.nodeProjects.linkDialog.projectDescriptionPlaceholder',
                        'Enter description...'
                      )}
                    />
                  </div>
                </TabsContent>
              </Tabs>
            </div>
          )}

          <DialogFooter>
            <Button
              variant="outline"
              onClick={() => {
                setLinkingProject(null);
                setSelectedSwarmProjectId(null);
                setNewProjectName('');
                setNewProjectDescription('');
                setDialogTab('existing');
              }}
            >
              {t('common:cancel', 'Cancel')}
            </Button>
            {dialogTab === 'existing' ? (
              <Button
                onClick={handleLink}
                disabled={
                  !selectedSwarmProjectId || mutations.linkNode.isPending
                }
              >
                {mutations.linkNode.isPending ? (
                  <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                ) : (
                  <Link2 className="h-4 w-4 mr-2" />
                )}
                {t(
                  'settings.swarm.nodeProjects.linkDialog.confirm',
                  'Link Project'
                )}
              </Button>
            ) : (
              <Button
                onClick={handleCreateAndLink}
                disabled={
                  !newProjectName.trim() ||
                  mutations.createProject.isPending ||
                  mutations.linkNode.isPending
                }
              >
                {mutations.createProject.isPending ||
                mutations.linkNode.isPending ? (
                  <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                ) : (
                  <Plus className="h-4 w-4 mr-2" />
                )}
                {t(
                  'settings.swarm.nodeProjects.linkDialog.createAndLink',
                  'Create & Link'
                )}
              </Button>
            )}
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}
