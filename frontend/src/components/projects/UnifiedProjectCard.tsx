import {
  Card,
  CardDescription,
  CardHeader,
  CardTitle,
} from '@/components/ui/card.tsx';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu.tsx';
import { Badge } from '@/components/ui/badge.tsx';
import { Button } from '@/components/ui/button.tsx';
import {
  Calendar,
  Circle,
  Cloud,
  Edit,
  ExternalLink,
  FolderOpen,
  Link2,
  MapPin,
  MoreHorizontal,
  Trash2,
  Unlink,
} from 'lucide-react';
import type { MergedProject, CachedNodeStatus, Project } from 'shared/types';
import { useEffect, useRef } from 'react';
import { useNavigateWithSearch } from '@/hooks';
import { projectsApi } from '@/lib/api';
import { LinkProjectDialog } from '@/components/dialogs/projects/LinkProjectDialog';
import { useTranslation } from 'react-i18next';
import { useProjectMutations } from '@/hooks/useProjectMutations';
import { ProjectEditorSelectionDialog } from '@/components/dialogs/projects/ProjectEditorSelectionDialog';

type Props = {
  project: MergedProject;
  isFocused: boolean;
  onRefresh: () => void;
  onEdit?: (project: MergedProject) => void;
};

function getStatusColor(status: CachedNodeStatus): string {
  switch (status) {
    case 'online':
      return 'text-green-500';
    case 'busy':
      return 'text-yellow-500';
    case 'offline':
      return 'text-gray-400';
    case 'draining':
      return 'text-orange-500';
    case 'pending':
    default:
      return 'text-gray-300';
  }
}

function UnifiedProjectCard({ project, isFocused, onRefresh, onEdit }: Props) {
  const navigate = useNavigateWithSearch();
  const ref = useRef<HTMLDivElement>(null);
  const { t } = useTranslation('projects');

  const { unlinkProject } = useProjectMutations({
    onUnlinkSuccess: () => {
      onRefresh();
    },
    onUnlinkError: (error) => {
      console.error('Failed to unlink project:', error);
    },
  });

  useEffect(() => {
    if (isFocused && ref.current) {
      ref.current.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
      ref.current.focus();
    }
  }, [isFocused]);

  const handleDelete = async () => {
    if (!project.has_local || !project.local_project_id) return;

    if (
      !confirm(
        `Are you sure you want to delete "${project.name}"? This action cannot be undone.`
      )
    )
      return;

    try {
      await projectsApi.delete(project.local_project_id);
      onRefresh();
    } catch (error) {
      console.error('Failed to delete project:', error);
    }
  };

  const handleEdit = () => {
    if (onEdit) {
      onEdit(project);
    }
  };

  const handleOpenInIDE = async () => {
    if (!project.has_local || !project.local_project_id) return;

    try {
      const response = await projectsApi.openEditor(project.local_project_id, {
        editor_type: null,
        file_path: null,
      });

      if (response.url) {
        window.open(response.url, '_blank');
      }
    } catch (err) {
      console.error('Failed to open project in editor:', err);
      // Show editor selection dialog on failure
      ProjectEditorSelectionDialog.show({
        selectedProject: {
          id: project.local_project_id,
          name: project.name,
          git_repo_path: project.git_repo_path,
          created_at: project.created_at,
          remote_project_id: project.remote_project_id,
        } as Project,
      });
    }
  };

  const handleLinkProject = async () => {
    if (!project.has_local || !project.local_project_id) return;

    try {
      await LinkProjectDialog.show({
        projectId: project.local_project_id,
        projectName: project.name,
      });
      onRefresh();
    } catch (error) {
      console.error('Failed to link project:', error);
    }
  };

  const handleUnlinkProject = () => {
    if (!project.has_local || !project.local_project_id) return;

    const confirmed = window.confirm(
      `Are you sure you want to unlink "${project.name}"? The local project will remain, but it will no longer be linked to the remote project.`
    );
    if (confirmed) {
      unlinkProject.mutate(project.local_project_id);
    }
  };

  const handleCardClick = () => {
    // Navigate using the main ID (local if available, otherwise first remote)
    navigate(`/projects/${project.id}/tasks`);
  };

  // Build location badges
  const locations: Array<{ name: string; status?: CachedNodeStatus }> = [];
  if (project.has_local) {
    locations.push({ name: 'local' });
  }
  project.nodes.forEach((node) => {
    locations.push({
      name: node.node_short_name,
      status: node.node_status,
    });
  });

  return (
    <Card
      className="hover:shadow-md transition-shadow cursor-pointer focus:ring-2 focus:ring-primary outline-none border"
      onClick={handleCardClick}
      tabIndex={isFocused ? 0 : -1}
      ref={ref}
    >
      <CardHeader>
        <div className="flex items-start justify-between">
          <div className="flex items-center gap-2 flex-wrap">
            <CardTitle className="text-lg">{project.name}</CardTitle>
            {project.remote_project_id && (
              <span
                className="inline-flex items-center gap-1 px-2 py-0.5 text-xs font-medium rounded-full bg-blue-50 text-blue-700 border border-blue-200 dark:bg-blue-950 dark:text-blue-300 dark:border-blue-800"
                title={t('linkedToOrganizationTooltip')}
              >
                <Cloud className="h-3 w-3" />
                {t('linked')}
              </span>
            )}
          </div>
          <div className="flex items-center gap-2">
            <DropdownMenu>
              <DropdownMenuTrigger asChild onClick={(e) => e.stopPropagation()}>
                <Button variant="ghost" size="sm" className="h-8 w-8 p-0">
                  <MoreHorizontal className="h-4 w-4" />
                </Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end">
                <DropdownMenuItem
                  onClick={(e) => {
                    e.stopPropagation();
                    navigate(`/projects/${project.id}`);
                  }}
                >
                  <ExternalLink className="mr-2 h-4 w-4" />
                  {t('viewProject')}
                </DropdownMenuItem>

                {project.has_local && (
                  <DropdownMenuItem
                    onClick={(e) => {
                      e.stopPropagation();
                      handleOpenInIDE();
                    }}
                  >
                    <FolderOpen className="mr-2 h-4 w-4" />
                    {t('openInIDE')}
                  </DropdownMenuItem>
                )}

                {/* Local project actions */}
                {project.has_local && (
                  <>
                    {project.remote_project_id ? (
                      <DropdownMenuItem
                        onClick={(e) => {
                          e.stopPropagation();
                          handleUnlinkProject();
                        }}
                      >
                        <Unlink className="mr-2 h-4 w-4" />
                        {t('unlinkFromOrganization')}
                      </DropdownMenuItem>
                    ) : (
                      <DropdownMenuItem
                        onClick={(e) => {
                          e.stopPropagation();
                          handleLinkProject();
                        }}
                      >
                        <Link2 className="mr-2 h-4 w-4" />
                        {t('linkToOrganization')}
                      </DropdownMenuItem>
                    )}
                    {onEdit && (
                      <DropdownMenuItem
                        onClick={(e) => {
                          e.stopPropagation();
                          handleEdit();
                        }}
                      >
                        <Edit className="mr-2 h-4 w-4" />
                        {t('common:buttons.edit')}
                      </DropdownMenuItem>
                    )}
                    <DropdownMenuItem
                      onClick={(e) => {
                        e.stopPropagation();
                        handleDelete();
                      }}
                      className="text-destructive"
                    >
                      <Trash2 className="mr-2 h-4 w-4" />
                      {t('common:buttons.delete')}
                    </DropdownMenuItem>
                  </>
                )}

                {/* Remote-only project actions */}
                {!project.has_local && (
                  <DropdownMenuItem
                    onClick={(e) => {
                      e.stopPropagation();
                      // TODO: Session 4 - Link to Local Folder dialog
                      console.log('Link to Local Folder - to be implemented');
                    }}
                  >
                    <Link2 className="mr-2 h-4 w-4" />
                    {t('linkToLocalFolder')}
                  </DropdownMenuItem>
                )}
              </DropdownMenuContent>
            </DropdownMenu>
          </div>
        </div>
        <CardDescription className="flex items-center gap-3 flex-wrap">
          <span className="flex items-center">
            <Calendar className="mr-1 h-3 w-3" />
            {t('createdDate', {
              date: new Date(project.created_at).toLocaleDateString(),
            })}
          </span>
          {locations.length > 0 && (
            <span className="flex items-center gap-1.5">
              <MapPin className="h-3 w-3" />
              {locations.map((loc) => (
                <Badge
                  key={loc.name}
                  variant="secondary"
                  className="gap-1 px-1.5 py-0 text-xs"
                >
                  {loc.name}
                  {loc.status && (
                    <Circle
                      className={`h-2 w-2 fill-current ${getStatusColor(loc.status)}`}
                    />
                  )}
                </Badge>
              ))}
            </span>
          )}
        </CardDescription>
      </CardHeader>
    </Card>
  );
}

export default UnifiedProjectCard;
