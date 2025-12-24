import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import {
  Cpu,
  HardDrive,
  Folder,
  Terminal,
  XCircle,
  ChevronRight,
} from 'lucide-react';
import type { ProcessInfo } from 'shared/types';
import { useTranslation } from 'react-i18next';
import { cn } from '@/lib/utils';

interface ProcessCardProps {
  process: ProcessInfo;
  onKill?: (pid: number) => void;
  isKilling?: boolean;
  onClick?: () => void;
  isChild?: boolean;
}

/**
 * Format bytes to human-readable string (KB, MB, GB)
 */
function formatBytes(bytes: bigint | number): string {
  const numBytes = typeof bytes === 'bigint' ? Number(bytes) : bytes;
  if (numBytes < 1024) return `${numBytes} B`;
  if (numBytes < 1024 * 1024) return `${(numBytes / 1024).toFixed(1)} KB`;
  if (numBytes < 1024 * 1024 * 1024)
    return `${(numBytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(numBytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

/**
 * Truncate long paths for display
 */
function truncatePath(path: string | null, maxLength = 40): string {
  if (!path) return '';
  if (path.length <= maxLength) return path;
  return '...' + path.slice(-maxLength);
}

export function ProcessCard({
  process,
  onKill,
  isKilling,
  onClick,
  isChild,
}: ProcessCardProps) {
  const { t } = useTranslation('processes');

  const handleKill = (e: React.MouseEvent) => {
    e.stopPropagation();
    if (onKill) {
      onKill(process.pid);
    }
  };

  return (
    <Card
      className={cn(
        'transition-shadow hover:shadow-md',
        onClick && 'cursor-pointer',
        isChild && 'ml-4 border-l-2 border-muted'
      )}
      onClick={onClick}
    >
      <CardHeader className="pb-2">
        <div className="flex items-start justify-between gap-2">
          <div className="flex items-center gap-2 min-w-0 flex-1">
            {isChild && (
              <ChevronRight className="h-4 w-4 text-muted-foreground shrink-0" />
            )}
            <Terminal className="h-4 w-4 text-muted-foreground shrink-0" />
            <CardTitle className="text-sm font-medium truncate">
              {process.name}
            </CardTitle>
          </div>
          <div className="flex items-center gap-2 shrink-0">
            {process.is_executor && (
              <Badge variant="default" className="text-xs">
                {t('badges.executor')}
              </Badge>
            )}
            <Badge variant="outline" className="text-xs font-mono">
              PID {process.pid}
            </Badge>
            {onKill && (
              <Button
                variant="ghost"
                size="icon"
                className="h-7 w-7 text-destructive hover:text-destructive hover:bg-destructive/10"
                onClick={handleKill}
                disabled={isKilling}
                title={t('actions.killProcess')}
              >
                <XCircle className="h-4 w-4" />
              </Button>
            )}
          </div>
        </div>
        {(process.project_name || process.task_title) && (
          <CardDescription className="text-xs mt-1">
            {process.project_name && (
              <span className="font-medium">{process.project_name}</span>
            )}
            {process.project_name && process.task_title && ' / '}
            {process.task_title && <span>{process.task_title}</span>}
          </CardDescription>
        )}
      </CardHeader>
      <CardContent className="pt-0">
        <div className="flex flex-wrap gap-x-4 gap-y-1 text-xs text-muted-foreground">
          <div className="flex items-center gap-1">
            <Cpu className="h-3 w-3" />
            <span>{process.cpu_percent.toFixed(1)}%</span>
          </div>
          <div className="flex items-center gap-1">
            <HardDrive className="h-3 w-3" />
            <span>{formatBytes(process.memory_bytes)}</span>
          </div>
          {process.working_directory && (
            <div className="flex items-center gap-1 min-w-0">
              <Folder className="h-3 w-3 shrink-0" />
              <span className="truncate" title={process.working_directory}>
                {truncatePath(process.working_directory)}
              </span>
            </div>
          )}
        </div>
        {process.command.length > 0 && (
          <div className="mt-2 text-xs font-mono text-muted-foreground bg-muted/50 p-2 rounded overflow-x-auto">
            <span className="whitespace-nowrap">
              {process.command.join(' ').slice(0, 100)}
              {process.command.join(' ').length > 100 && '...'}
            </span>
          </div>
        )}
      </CardContent>
    </Card>
  );
}

export default ProcessCard;
