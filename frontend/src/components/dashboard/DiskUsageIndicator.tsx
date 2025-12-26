import { HardDrive, ChevronUp, Loader2 } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { useState, useEffect } from 'react';
import { Card } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { useDiskUsage } from '@/hooks/useDiskUsage';
import { cn } from '@/lib/utils';

const BANNER_OPEN_KEY = 'disk-usage-banner-open';

/**
 * Format bytes to human-readable string.
 */
function formatBytes(bytes: bigint): string {
  const KB = 1024n;
  const MB = KB * 1024n;
  const GB = MB * 1024n;

  if (bytes >= GB) {
    return `${(Number(bytes) / Number(GB)).toFixed(2)} GB`;
  } else if (bytes >= MB) {
    return `${(Number(bytes) / Number(MB)).toFixed(2)} MB`;
  } else if (bytes >= KB) {
    return `${(Number(bytes) / Number(KB)).toFixed(2)} KB`;
  }
  return `${bytes} B`;
}

/**
 * Get color class based on disk usage.
 */
function getUsageColor(bytes: bigint): string {
  const GB = 1024n * 1024n * 1024n;
  if (bytes >= 50n * GB) {
    return 'text-destructive'; // Over 50GB - critical
  } else if (bytes >= 20n * GB) {
    return 'text-amber-500 dark:text-amber-400'; // Over 20GB - warning
  }
  return 'text-muted-foreground'; // Normal
}

function WorktreeItem({
  name,
  bytes,
}: {
  name: string;
  bytes: bigint;
}) {
  return (
    <div className="flex items-center justify-between py-1 px-2 hover:bg-accent/50 rounded-md transition-colors">
      <span className="text-sm font-mono truncate flex-1">{name}</span>
      <span className="text-sm text-muted-foreground ml-2 shrink-0">
        {formatBytes(bytes)}
      </span>
    </div>
  );
}

export function DiskUsageIndicator() {
  const { t } = useTranslation('common');
  const { data, isLoading, error } = useDiskUsage();
  const [isOpen, setIsOpen] = useState(() => {
    const stored = localStorage.getItem(BANNER_OPEN_KEY);
    return stored === 'true'; // Default to closed
  });

  useEffect(() => {
    localStorage.setItem(BANNER_OPEN_KEY, String(isOpen));
  }, [isOpen]);

  if (isLoading) {
    return (
      <Card className="bg-muted p-3 text-sm flex items-center gap-3">
        <HardDrive className="h-4 w-4 text-muted-foreground" />
        <span className="text-muted-foreground">
          {t('diskUsage.loading', 'Loading disk usage...')}
        </span>
        <Loader2 className="h-4 w-4 animate-spin text-muted-foreground ml-auto" />
      </Card>
    );
  }

  if (error || !data) {
    return (
      <Card className="bg-muted p-3 text-sm flex items-center gap-3">
        <HardDrive className="h-4 w-4 text-muted-foreground" />
        <span className="text-muted-foreground">
          {t('diskUsage.error', 'Failed to load disk usage')}
        </span>
      </Card>
    );
  }

  const usageColor = getUsageColor(data.used_bytes);

  return (
    <details
      className="group"
      open={isOpen}
      onToggle={(e) => setIsOpen(e.currentTarget.open)}
    >
      <summary className="list-none cursor-pointer">
        <Card className="bg-muted p-3 text-sm flex items-center justify-between">
          <div className="flex items-center gap-3">
            <HardDrive className={cn('h-4 w-4', usageColor)} />
            <span className="font-medium">
              {t('diskUsage.title', 'Worktree Disk Usage')}
            </span>
            <Badge
              variant="secondary"
              className={cn(usageColor, 'font-mono')}
            >
              {formatBytes(data.used_bytes)}
            </Badge>
            <Badge variant="outline" className="text-xs">
              {data.worktree_count}{' '}
              {t('diskUsage.worktrees', 'worktrees')}
            </Badge>
          </div>
          <ChevronUp
            aria-hidden
            className="h-4 w-4 text-muted-foreground transition-transform group-open:rotate-180"
          />
        </Card>
      </summary>

      <Card className="mt-2 p-3">
        <div className="text-xs text-muted-foreground mb-2 px-2">
          {t('diskUsage.largestWorktrees', 'Largest worktrees')}:
        </div>
        <div className="space-y-0.5">
          {data.largest_worktrees.length === 0 ? (
            <div className="text-sm text-muted-foreground px-2 py-1">
              {t('diskUsage.noWorktrees', 'No worktrees found')}
            </div>
          ) : (
            data.largest_worktrees.map((wt) => (
              <WorktreeItem key={wt.name} name={wt.name} bytes={wt.bytes} />
            ))
          )}
        </div>
        <div className="text-xs text-muted-foreground mt-3 px-2 border-t border-border pt-2">
          {t('diskUsage.directory', 'Directory')}: <code className="text-xs">{data.worktree_dir}</code>
        </div>
      </Card>
    </details>
  );
}
