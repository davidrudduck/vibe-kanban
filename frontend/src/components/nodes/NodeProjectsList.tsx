import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Skeleton } from '@/components/ui/skeleton';
import { Folder, GitBranch, RefreshCw, Clock } from 'lucide-react';
import { useNodeProjects } from '@/hooks/useNodeProjects';
import type { NodeProject } from '@/types/nodes';
import { formatDistanceToNow } from 'date-fns';

interface NodeProjectsListProps {
  nodeId: string;
}

const syncStatusConfig: Record<
  string,
  {
    label: string;
    variant: 'default' | 'secondary' | 'destructive' | 'outline';
  }
> = {
  synced: { label: 'Synced', variant: 'secondary' },
  pending: { label: 'Pending', variant: 'outline' },
  syncing: { label: 'Syncing', variant: 'default' },
  error: { label: 'Error', variant: 'destructive' },
};

export function NodeProjectsList({ nodeId }: NodeProjectsListProps) {
  const { data: projects, isLoading, isError, error } = useNodeProjects(nodeId);

  if (isLoading) {
    return (
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <Folder className="h-5 w-5" />
            Projects
          </CardTitle>
          <CardDescription>Projects registered on this node</CardDescription>
        </CardHeader>
        <CardContent>
          <div className="space-y-3">
            {[1, 2, 3].map((i) => (
              <div key={i} className="flex items-center gap-3">
                <Skeleton className="h-10 w-10 rounded" />
                <div className="flex-1 space-y-2">
                  <Skeleton className="h-4 w-1/3" />
                  <Skeleton className="h-3 w-1/2" />
                </div>
              </div>
            ))}
          </div>
        </CardContent>
      </Card>
    );
  }

  if (isError) {
    return (
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <Folder className="h-5 w-5" />
            Projects
          </CardTitle>
        </CardHeader>
        <CardContent>
          <p className="text-destructive text-sm">
            Failed to load projects: {error?.message || 'Unknown error'}
          </p>
        </CardContent>
      </Card>
    );
  }

  const projectCount = projects?.length || 0;

  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <Folder className="h-5 w-5" />
          Projects
          <Badge variant="secondary" className="ml-2">
            {projectCount}
          </Badge>
        </CardTitle>
        <CardDescription>Projects registered on this node</CardDescription>
      </CardHeader>
      <CardContent>
        {projectCount === 0 ? (
          <p className="text-muted-foreground text-sm text-center py-4">
            No projects registered on this node
          </p>
        ) : (
          <div className="space-y-3">
            {projects?.map((project) => (
              <ProjectItem key={project.id} project={project} />
            ))}
          </div>
        )}
      </CardContent>
    </Card>
  );
}

interface ProjectItemProps {
  project: NodeProject;
}

function ProjectItem({ project }: ProjectItemProps) {
  const statusConfig = syncStatusConfig[project.sync_status] || {
    label: project.sync_status,
    variant: 'outline' as const,
  };

  const lastSynced = project.last_synced_at
    ? formatDistanceToNow(new Date(project.last_synced_at), { addSuffix: true })
    : null;

  return (
    <div className="flex items-start justify-between p-3 border rounded-lg">
      <div className="flex items-start gap-3 min-w-0 flex-1">
        <div className="p-2 bg-secondary rounded">
          <Folder className="h-4 w-4 text-muted-foreground" />
        </div>
        <div className="min-w-0 flex-1">
          <div className="font-medium text-sm truncate">
            {project.git_repo_path}
          </div>
          <div className="flex items-center gap-2 text-xs text-muted-foreground mt-1">
            <GitBranch className="h-3 w-3" />
            <span>{project.default_branch}</span>
          </div>
          {lastSynced && (
            <div className="flex items-center gap-1 text-xs text-muted-foreground mt-1">
              <Clock className="h-3 w-3" />
              <span>Synced {lastSynced}</span>
            </div>
          )}
        </div>
      </div>
      <div className="flex items-center gap-2 flex-shrink-0">
        {project.sync_status === 'syncing' && (
          <RefreshCw className="h-4 w-4 animate-spin text-muted-foreground" />
        )}
        <Badge variant={statusConfig.variant}>{statusConfig.label}</Badge>
      </div>
    </div>
  );
}
