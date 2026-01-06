import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Skeleton } from '@/components/ui/skeleton';
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from '@/components/ui/tooltip';
import { Folder, GitBranch, Clock, Link2, AlertCircle } from 'lucide-react';
import { useNodeProjects } from '@/hooks/useNodeProjects';
import type { NodeProject } from '@/types/nodes';
import { formatDistanceToNow, differenceInMinutes } from 'date-fns';

/** Threshold in minutes after which a project is considered stale */
const STALE_THRESHOLD_MINUTES = 10;

/** Check if a project is stale based on last_seen_at */
function isProjectStale(lastSeenAt: string): boolean {
  const lastSeen = new Date(lastSeenAt);
  const minutesAgo = differenceInMinutes(new Date(), lastSeen);
  return minutesAgo > STALE_THRESHOLD_MINUTES;
}

interface NodeProjectsListProps {
  nodeId: string;
}

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
  const lastSeen = project.last_seen_at
    ? formatDistanceToNow(new Date(project.last_seen_at), { addSuffix: true })
    : null;

  const isLinked = !!project.swarm_project_id;
  const stale = isProjectStale(project.last_seen_at);

  return (
    <div
      className={`flex items-start justify-between p-3 border rounded-lg ${
        stale ? 'opacity-60' : ''
      }`}
    >
      <div className="flex items-start gap-3 min-w-0 flex-1">
        <div className="p-2 bg-secondary rounded">
          <Folder className="h-4 w-4 text-muted-foreground" />
        </div>
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <span className="font-medium text-sm truncate">{project.name}</span>
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
                    Last seen {lastSeen}. Node may have removed this project.
                  </p>
                </TooltipContent>
              </Tooltip>
            )}
          </div>
          <div className="text-xs text-muted-foreground truncate">
            {project.git_repo_path}
          </div>
          <div className="flex items-center gap-2 text-xs text-muted-foreground mt-1">
            <GitBranch className="h-3 w-3" />
            <span>{project.default_branch}</span>
          </div>
          {lastSeen && !stale && (
            <div className="flex items-center gap-1 text-xs text-muted-foreground mt-1">
              <Clock className="h-3 w-3" />
              <span>Seen {lastSeen}</span>
            </div>
          )}
        </div>
      </div>
      <div className="flex items-center gap-2 flex-shrink-0">
        {isLinked ? (
          <Badge variant="secondary" className="flex items-center gap-1">
            <Link2 className="h-3 w-3" />
            {project.swarm_project_name}
          </Badge>
        ) : (
          <Badge variant="outline">Unlinked</Badge>
        )}
      </div>
    </div>
  );
}
