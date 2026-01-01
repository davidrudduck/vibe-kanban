import { useTranslation } from 'react-i18next';
import {
  ChevronDown,
  ChevronRight,
  Edit2,
  Trash2,
  Unlink,
  FolderGit2,
  Monitor,
} from 'lucide-react';
import { Button } from '@/components/ui/button';
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from '@/components/ui/tooltip';
import { Badge } from '@/components/ui/badge';
import { cn } from '@/lib/utils';
import type { SwarmProjectWithNodes, SwarmProjectNode } from '@/types/swarm';
import { formatDistanceToNow } from 'date-fns';

interface SwarmProjectRowProps {
  project: SwarmProjectWithNodes;
  nodes: SwarmProjectNode[];
  isLoadingNodes: boolean;
  isExpanded: boolean;
  onToggleExpand: () => void;
  onEdit: () => void;
  onDelete: () => void;
  onUnlinkNode: (nodeId: string) => void;
}

export function SwarmProjectRow({
  project,
  nodes,
  isLoadingNodes,
  isExpanded,
  onToggleExpand,
  onEdit,
  onDelete,
  onUnlinkNode,
}: SwarmProjectRowProps) {
  const { t } = useTranslation(['settings', 'common']);

  const getOsIcon = (osType: string | null) => {
    if (!osType) return null;
    const os = osType.toLowerCase();
    if (os.includes('darwin') || os.includes('mac')) return 'macOS';
    if (os.includes('linux')) return 'Linux';
    if (os.includes('win')) return 'Windows';
    return osType;
  };

  return (
    <div className="border-b border-border last:border-b-0">
      {/* Main Row */}
      <div
        className={cn(
          'flex items-center gap-2 px-3 py-3 sm:px-4',
          'hover:bg-muted/50 transition-colors cursor-pointer'
        )}
        onClick={onToggleExpand}
        role="button"
        tabIndex={0}
        aria-expanded={isExpanded}
        onKeyDown={(e) => {
          if (e.key === 'Enter' || e.key === ' ') {
            e.preventDefault();
            onToggleExpand();
          }
        }}
      >
        {/* Expand Icon */}
        <div className="shrink-0">
          {isExpanded ? (
            <ChevronDown className="h-4 w-4 text-muted-foreground" />
          ) : (
            <ChevronRight className="h-4 w-4 text-muted-foreground" />
          )}
        </div>

        {/* Project Icon */}
        <FolderGit2 className="h-5 w-5 text-muted-foreground shrink-0" />

        {/* Project Name */}
        <div className="flex-1 min-w-0">
          <div className="font-medium truncate">{project.name}</div>
          {project.description && (
            <div className="text-sm text-muted-foreground truncate">
              {project.description}
            </div>
          )}
        </div>

        {/* Linked Nodes Count Badge */}
        <Badge variant="secondary" className="shrink-0 hidden sm:inline-flex">
          {project.linked_nodes_count}{' '}
          {project.linked_nodes_count === 1
            ? t('settings.swarm.projects.node', 'node')
            : t('settings.swarm.projects.nodes', 'nodes')}
        </Badge>

        {/* Mobile node count */}
        <Badge variant="secondary" className="shrink-0 sm:hidden">
          {project.linked_nodes_count}
        </Badge>

        {/* Actions */}
        <div
          className="flex items-center gap-1 shrink-0"
          onClick={(e) => e.stopPropagation()}
        >
          <Tooltip>
            <TooltipTrigger asChild>
              <Button
                variant="ghost"
                size="sm"
                className="h-8 w-8 p-0"
                onClick={onEdit}
              >
                <Edit2 className="h-4 w-4" />
                <span className="sr-only">
                  {t('settings.swarm.projects.edit', 'Edit')}
                </span>
              </Button>
            </TooltipTrigger>
            <TooltipContent>
              {t('settings.swarm.projects.editTooltip', 'Edit project')}
            </TooltipContent>
          </Tooltip>

          <Tooltip>
            <TooltipTrigger asChild>
              <Button
                variant="ghost"
                size="sm"
                className="h-8 w-8 p-0 text-destructive hover:text-destructive"
                onClick={onDelete}
              >
                <Trash2 className="h-4 w-4" />
                <span className="sr-only">
                  {t('settings.swarm.projects.delete', 'Delete')}
                </span>
              </Button>
            </TooltipTrigger>
            <TooltipContent>
              {t('settings.swarm.projects.deleteTooltip', 'Delete project')}
            </TooltipContent>
          </Tooltip>
        </div>
      </div>

      {/* Expanded Node Details */}
      {isExpanded && (
        <div className="bg-muted/30 border-t border-border">
          {isLoadingNodes ? (
            <div className="px-4 py-3 text-sm text-muted-foreground">
              {t(
                'settings.swarm.projects.loadingNodes',
                'Loading linked nodes...'
              )}
            </div>
          ) : nodes.length === 0 ? (
            <div className="px-4 py-3 text-sm text-muted-foreground">
              {t(
                'settings.swarm.projects.noNodes',
                'No nodes linked to this project yet.'
              )}
            </div>
          ) : (
            <div className="divide-y divide-border">
              {nodes.map((node) => (
                <div
                  key={node.id}
                  className="flex items-center gap-3 px-4 py-2 sm:px-6"
                >
                  <Monitor className="h-4 w-4 text-muted-foreground shrink-0" />
                  <div className="flex-1 min-w-0">
                    <div className="text-sm font-medium truncate">
                      {node.git_repo_path}
                    </div>
                    <div className="flex items-center gap-2 text-xs text-muted-foreground">
                      {node.os_type && <span>{getOsIcon(node.os_type)}</span>}
                      <span>
                        {t('settings.swarm.projects.linkedAt', 'Linked')}{' '}
                        {formatDistanceToNow(new Date(node.linked_at), {
                          addSuffix: true,
                        })}
                      </span>
                    </div>
                  </div>
                  <Tooltip>
                    <TooltipTrigger asChild>
                      <Button
                        variant="ghost"
                        size="sm"
                        className="h-7 w-7 p-0"
                        onClick={() => onUnlinkNode(node.node_id)}
                      >
                        <Unlink className="h-3.5 w-3.5" />
                        <span className="sr-only">
                          {t('settings.swarm.projects.unlink', 'Unlink')}
                        </span>
                      </Button>
                    </TooltipTrigger>
                    <TooltipContent>
                      {t(
                        'settings.swarm.projects.unlinkTooltip',
                        'Unlink this node'
                      )}
                    </TooltipContent>
                  </Tooltip>
                </div>
              ))}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
