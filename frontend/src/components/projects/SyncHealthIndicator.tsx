import { AlertTriangle } from 'lucide-react';
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from '@/components/ui/tooltip';
import { useProjectSyncHealth } from '@/hooks/useProjectSyncHealth';

type Props = {
  projectId: string;
};

export function SyncHealthIndicator({ projectId }: Props) {
  const { data: syncHealth, isLoading } = useProjectSyncHealth(projectId);

  // Don't render anything while loading or if no sync issues
  if (isLoading || !syncHealth?.has_sync_issues) {
    return null;
  }

  // Build tooltip content from issues
  const tooltipContent = (
    <div className="space-y-1">
      <div className="font-semibold">Sync Issues Detected</div>
      {syncHealth.issues.map((issue, index) => {
        if (issue.type === 'orphaned_tasks') {
          return (
            <div key={index} className="text-sm">
              {Number(issue.count)} orphaned task
              {Number(issue.count) !== 1 ? 's' : ''}
            </div>
          );
        } else if (issue.type === 'project_not_linked') {
          return (
            <div key={index} className="text-sm">
              Project not linked to Hive
            </div>
          );
        }
        return null;
      })}
      {Number(syncHealth.orphaned_task_count) > 0 && (
        <div className="text-xs text-muted-foreground mt-1">
          Total orphaned tasks: {Number(syncHealth.orphaned_task_count)}
        </div>
      )}
    </div>
  );

  return (
    <TooltipProvider>
      <Tooltip>
        <TooltipTrigger asChild>
          <span className="inline-flex items-center">
            <AlertTriangle className="h-4 w-4 text-amber-500" />
          </span>
        </TooltipTrigger>
        <TooltipContent>{tooltipContent}</TooltipContent>
      </Tooltip>
    </TooltipProvider>
  );
}
