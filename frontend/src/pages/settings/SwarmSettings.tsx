import { useState, useMemo } from 'react';
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Label } from '@/components/ui/label';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { Badge } from '@/components/ui/badge';
import {
  Loader2,
  Network,
  Link2,
  Unlink,
  FolderOpen,
  Server,
  CheckCircle2,
  XCircle,
  AlertCircle,
} from 'lucide-react';
import { useUserOrganizations } from '@/hooks/useUserOrganizations';
import { useOrganizationSelection } from '@/hooks/useOrganizationSelection';
import { useProjects } from '@/hooks/useProjects';
import { useOrganizationProjects } from '@/hooks/useOrganizationProjects';
import { useProjectMutations } from '@/hooks/useProjectMutations';
import { useUserSystem } from '@/components/ConfigProvider';
import { useAuth } from '@/hooks/auth/useAuth';
import { useFeedback } from '@/hooks/useFeedback';
import { LoginRequiredPrompt } from '@/components/dialogs/shared/LoginRequiredPrompt';
import { useTranslation } from 'react-i18next';
import { cn } from '@/lib/utils';

// Component for connection status indicator
function ConnectionStatus({
  isConnected,
  nodeName,
}: {
  isConnected: boolean;
  nodeName?: string;
}) {
  const { t } = useTranslation('settings');

  return (
    <div className="flex items-center gap-2">
      <div
        className={cn(
          'h-3 w-3 rounded-full',
          isConnected ? 'bg-green-500' : 'bg-red-500'
        )}
      />
      <span className="text-sm">
        {isConnected ? (
          <>
            {t('settings.swarm.connection.connected')}
            {nodeName && (
              <span className="text-muted-foreground ml-1">({nodeName})</span>
            )}
          </>
        ) : (
          t('settings.swarm.connection.disconnected')
        )}
      </span>
    </div>
  );
}

// Component for swarm project row
interface SwarmProjectRowProps {
  remoteProjectId: string;
  remoteProjectName: string;
  linkedLocalProject: {
    id: string;
    name: string;
    git_repo_path: string;
  } | null;
  availableLocalProjects: Array<{
    id: string;
    name: string;
    git_repo_path: string;
  }>;
  onLink: (remoteProjectId: string, localProjectId: string) => void;
  onUnlink: (localProjectId: string) => void;
  isLinking: boolean;
  isUnlinking: boolean;
}

function SwarmProjectRow({
  remoteProjectId,
  remoteProjectName,
  linkedLocalProject,
  availableLocalProjects,
  onLink,
  onUnlink,
  isLinking,
  isUnlinking,
}: SwarmProjectRowProps) {
  const { t } = useTranslation('settings');
  const [selectedProjectId, setSelectedProjectId] = useState<string>('');

  const handleLink = () => {
    if (selectedProjectId) {
      onLink(remoteProjectId, selectedProjectId);
      setSelectedProjectId('');
    }
  };

  return (
    <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-3 p-4 bg-muted/30 rounded-lg">
      <div className="flex items-start gap-3">
        <Network className="h-5 w-5 mt-0.5 text-muted-foreground shrink-0" />
        <div className="min-w-0">
          <div className="font-medium truncate">{remoteProjectName}</div>
          {linkedLocalProject ? (
            <div className="flex items-center gap-2 text-sm text-muted-foreground">
              <CheckCircle2 className="h-3.5 w-3.5 text-green-500" />
              <span className="truncate">{linkedLocalProject.name}</span>
              <span className="text-xs truncate hidden sm:inline">
                ({linkedLocalProject.git_repo_path})
              </span>
            </div>
          ) : (
            <div className="flex items-center gap-2 text-sm text-muted-foreground">
              <XCircle className="h-3.5 w-3.5 text-amber-500" />
              <span>{t('settings.swarm.projects.notLinked')}</span>
            </div>
          )}
        </div>
      </div>

      <div className="flex items-center gap-2 shrink-0">
        {linkedLocalProject ? (
          <Button
            variant="outline"
            size="sm"
            onClick={() => onUnlink(linkedLocalProject.id)}
            disabled={isUnlinking}
          >
            {isUnlinking ? (
              <Loader2 className="h-4 w-4 mr-1 animate-spin" />
            ) : (
              <Unlink className="h-4 w-4 mr-1" />
            )}
            {t('settings.swarm.projects.unlink')}
          </Button>
        ) : (
          <>
            <Select
              value={selectedProjectId}
              onValueChange={setSelectedProjectId}
            >
              <SelectTrigger className="w-[180px]">
                <SelectValue
                  placeholder={t('settings.swarm.projects.selectProject')}
                />
              </SelectTrigger>
              <SelectContent>
                {availableLocalProjects.length > 0 ? (
                  availableLocalProjects.map((project) => (
                    <SelectItem key={project.id} value={project.id}>
                      {project.name}
                    </SelectItem>
                  ))
                ) : (
                  <SelectItem value="none" disabled>
                    {t('settings.swarm.projects.noAvailable')}
                  </SelectItem>
                )}
              </SelectContent>
            </Select>
            <Button
              variant="default"
              size="sm"
              onClick={handleLink}
              disabled={!selectedProjectId || isLinking}
            >
              {isLinking ? (
                <Loader2 className="h-4 w-4 mr-1 animate-spin" />
              ) : (
                <Link2 className="h-4 w-4 mr-1" />
              )}
              {t('settings.swarm.projects.link')}
            </Button>
          </>
        )}
      </div>
    </div>
  );
}

// Component for local project row (not linked to any swarm project)
interface LocalProjectRowProps {
  project: {
    id: string;
    name: string;
    git_repo_path: string;
    swarm_project_id: string | null;
  };
  linkedSwarmProjectName?: string;
}

function LocalProjectRow({
  project,
  linkedSwarmProjectName,
}: LocalProjectRowProps) {
  const { t } = useTranslation('settings');

  return (
    <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-3 p-4 bg-muted/30 rounded-lg">
      <div className="flex items-start gap-3">
        <FolderOpen className="h-5 w-5 mt-0.5 text-muted-foreground shrink-0" />
        <div className="min-w-0">
          <div className="font-medium truncate">{project.name}</div>
          <div className="text-sm text-muted-foreground truncate">
            {project.git_repo_path}
          </div>
        </div>
      </div>

      <div className="flex items-center gap-2 shrink-0">
        {linkedSwarmProjectName ? (
          <Badge variant="secondary" className="gap-1">
            <Network className="h-3 w-3" />
            {linkedSwarmProjectName}
          </Badge>
        ) : (
          <Badge variant="outline" className="text-muted-foreground">
            {t('settings.swarm.localProjects.localOnly')}
          </Badge>
        )}
      </div>
    </div>
  );
}

export function SwarmSettings() {
  const { t } = useTranslation('settings');
  const { loginStatus } = useUserSystem();
  const { isSignedIn, isLoaded } = useAuth();
  const [error, setError] = useState<string | null>(null);
  const { success, showSuccess, clearSuccess } = useFeedback();

  // Fetch all organizations
  const {
    data: orgsResponse,
    isLoading: orgsLoading,
    error: orgsError,
  } = useUserOrganizations();

  // Organization selection with URL sync
  const { selectedOrgId, selectedOrg, handleOrgSelect } =
    useOrganizationSelection({
      organizations: orgsResponse,
      onSelectionChange: () => {
        clearSuccess();
        setError(null);
      },
    });

  // Fetch all local projects
  const { data: allProjects = [], isLoading: loadingProjects } = useProjects();

  // Fetch remote/swarm projects for the selected organization
  const { data: remoteProjects = [], isLoading: loadingRemoteProjects } =
    useOrganizationProjects(selectedOrgId);

  // Calculate available local projects (not linked to any remote project in this org)
  const remoteProjectIds = remoteProjects.map((rp) => rp.id);
  const availableLocalProjects = useMemo(() => {
    return allProjects.filter((project) => {
      // Project is available if it has no remote link OR if it's linked to a project outside this org
      return (
        !project.swarm_project_id ||
        !remoteProjectIds.includes(project.swarm_project_id)
      );
    });
  }, [allProjects, remoteProjectIds]);

  // Create a map of swarm project ids to names for display
  const swarmProjectNames = useMemo(() => {
    const map = new Map<string, string>();
    remoteProjects.forEach((rp) => map.set(rp.id, rp.name));
    return map;
  }, [remoteProjects]);

  // Project mutations
  const { linkToExisting, unlinkProject } = useProjectMutations({
    onLinkSuccess: () => {
      showSuccess(t('settings.swarm.success.linked'));
    },
    onLinkError: (err) => {
      setError(
        err instanceof Error ? err.message : t('settings.swarm.errors.linkFailed')
      );
    },
    onUnlinkSuccess: () => {
      showSuccess(t('settings.swarm.success.unlinked'));
    },
    onUnlinkError: (err) => {
      setError(
        err instanceof Error ? err.message : t('settings.swarm.errors.unlinkFailed')
      );
    },
  });

  const handleLinkProject = (
    remoteProjectId: string,
    localProjectId: string
  ) => {
    setError(null);
    linkToExisting.mutate({
      localProjectId,
      data: { swarm_project_id: remoteProjectId },
    });
  };

  const handleUnlinkProject = (projectId: string) => {
    setError(null);
    unlinkProject.mutate(projectId);
  };

  // Check connection status (simplified - just checking if we have organizations loaded)
  const isConnected = isSignedIn && !!selectedOrg;
  const nodeName = loginStatus?.status === 'loggedin' ? (loginStatus.profile.username ?? undefined) : undefined;

  if (!isLoaded || orgsLoading) {
    return (
      <div className="flex items-center justify-center py-8">
        <Loader2 className="h-8 w-8 animate-spin" />
        <span className="ml-2">{t('settings.swarm.loading')}</span>
      </div>
    );
  }

  if (!isSignedIn) {
    return (
      <div className="py-8">
        <LoginRequiredPrompt
          title={t('settings.swarm.loginRequired.title')}
          description={t('settings.swarm.loginRequired.description')}
          actionLabel={t('settings.swarm.loginRequired.action')}
        />
      </div>
    );
  }

  if (orgsError) {
    return (
      <div className="py-8">
        <Alert variant="destructive">
          <AlertDescription>
            {orgsError instanceof Error
              ? orgsError.message
              : t('settings.swarm.errors.loadFailed')}
          </AlertDescription>
        </Alert>
      </div>
    );
  }

  const organizations = orgsResponse?.organizations ?? [];

  return (
    <div className="space-y-6">
      {error && (
        <Alert variant="destructive">
          <AlertCircle className="h-4 w-4" />
          <AlertDescription>{error}</AlertDescription>
        </Alert>
      )}

      {success && (
        <Alert variant="success">
          <CheckCircle2 className="h-4 w-4" />
          <AlertDescription className="font-medium">{success}</AlertDescription>
        </Alert>
      )}

      {/* Connection Status Card */}
      <Card>
        <CardHeader>
          <div className="flex items-center justify-between">
            <div>
              <CardTitle className="flex items-center gap-2">
                <Server className="h-5 w-5" />
                {t('settings.swarm.connection.title')}
              </CardTitle>
              <CardDescription>
                {t('settings.swarm.connection.description')}
              </CardDescription>
            </div>
            <ConnectionStatus isConnected={isConnected} nodeName={nodeName} />
          </div>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="space-y-2">
            <Label htmlFor="org-selector">{t('settings.swarm.organization.label')}</Label>
            <Select value={selectedOrgId} onValueChange={handleOrgSelect}>
              <SelectTrigger id="org-selector">
                <SelectValue
                  placeholder={t('settings.swarm.organization.placeholder')}
                />
              </SelectTrigger>
              <SelectContent>
                {organizations.length > 0 ? (
                  organizations.map((org) => (
                    <SelectItem key={org.id} value={org.id}>
                      {org.name}
                    </SelectItem>
                  ))
                ) : (
                  <SelectItem value="no-orgs" disabled>
                    {t('settings.swarm.organization.noOrganizations')}
                  </SelectItem>
                )}
              </SelectContent>
            </Select>
            <p className="text-sm text-muted-foreground">
              {t('settings.swarm.organization.helper')}
            </p>
          </div>
        </CardContent>
      </Card>

      {/* Swarm Projects Card */}
      {selectedOrg && (
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <Network className="h-5 w-5" />
              {t('settings.swarm.projects.title')}
            </CardTitle>
            <CardDescription>
              {t('settings.swarm.projects.description', { orgName: selectedOrg.name })}
            </CardDescription>
          </CardHeader>
          <CardContent>
            {loadingProjects || loadingRemoteProjects ? (
              <div className="flex items-center justify-center py-8">
                <Loader2 className="h-6 w-6 animate-spin" />
                <span className="ml-2">{t('settings.swarm.projects.loading')}</span>
              </div>
            ) : remoteProjects.length === 0 ? (
              <div className="text-center py-8 text-muted-foreground">
                <Network className="h-12 w-12 mx-auto mb-4 opacity-50" />
                <p>{t('settings.swarm.projects.noSwarmProjects')}</p>
                <p className="text-sm mt-2">
                  {t('settings.swarm.projects.noSwarmProjectsHint')}
                </p>
              </div>
            ) : (
              <div className="space-y-3">
                {remoteProjects.map((remoteProject) => {
                  const linkedLocalProject = allProjects.find(
                    (p) => p.swarm_project_id === remoteProject.id
                  );

                  return (
                    <SwarmProjectRow
                      key={remoteProject.id}
                      remoteProjectId={remoteProject.id}
                      remoteProjectName={remoteProject.name}
                      linkedLocalProject={
                        linkedLocalProject
                          ? {
                              id: linkedLocalProject.id,
                              name: linkedLocalProject.name,
                              git_repo_path: linkedLocalProject.git_repo_path,
                            }
                          : null
                      }
                      availableLocalProjects={availableLocalProjects.map(
                        (p) => ({
                          id: p.id,
                          name: p.name,
                          git_repo_path: p.git_repo_path,
                        })
                      )}
                      onLink={handleLinkProject}
                      onUnlink={handleUnlinkProject}
                      isLinking={linkToExisting.isPending}
                      isUnlinking={unlinkProject.isPending}
                    />
                  );
                })}
              </div>
            )}
          </CardContent>
        </Card>
      )}

      {/* Local Projects Card */}
      {selectedOrg && (
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <FolderOpen className="h-5 w-5" />
              {t('settings.swarm.localProjects.title')}
            </CardTitle>
            <CardDescription>
              {t('settings.swarm.localProjects.description')}
            </CardDescription>
          </CardHeader>
          <CardContent>
            {loadingProjects ? (
              <div className="flex items-center justify-center py-8">
                <Loader2 className="h-6 w-6 animate-spin" />
                <span className="ml-2">{t('settings.swarm.localProjects.loading')}</span>
              </div>
            ) : allProjects.length === 0 ? (
              <div className="text-center py-8 text-muted-foreground">
                <FolderOpen className="h-12 w-12 mx-auto mb-4 opacity-50" />
                <p>{t('settings.swarm.localProjects.noProjects')}</p>
              </div>
            ) : (
              <div className="space-y-3">
                {allProjects.map((project) => (
                  <LocalProjectRow
                    key={project.id}
                    project={project}
                    linkedSwarmProjectName={
                      project.swarm_project_id
                        ? swarmProjectNames.get(project.swarm_project_id)
                        : undefined
                    }
                  />
                ))}
              </div>
            )}
          </CardContent>
        </Card>
      )}

      {/* Information Card */}
      <Card>
        <CardHeader>
          <CardTitle>{t('settings.swarm.info.title')}</CardTitle>
        </CardHeader>
        <CardContent className="space-y-3">
          <div className="flex items-start gap-3">
            <Link2 className="h-5 w-5 mt-0.5 text-muted-foreground shrink-0" />
            <div>
              <p className="font-medium">{t('settings.swarm.info.linkingTitle')}</p>
              <p className="text-sm text-muted-foreground">
                {t('settings.swarm.info.linkingDescription')}
              </p>
            </div>
          </div>
          <div className="flex items-start gap-3">
            <Network className="h-5 w-5 mt-0.5 text-muted-foreground shrink-0" />
            <div>
              <p className="font-medium">{t('settings.swarm.info.syncTitle')}</p>
              <p className="text-sm text-muted-foreground">
                {t('settings.swarm.info.syncDescription')}
              </p>
            </div>
          </div>
        </CardContent>
      </Card>
    </div>
  );
}
