import { useState, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { useQuery } from '@tanstack/react-query';
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from '@/components/ui/card';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { Button } from '@/components/ui/button';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Label } from '@/components/ui/label';
import { Badge } from '@/components/ui/badge';
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
  TooltipProvider,
} from '@/components/ui/tooltip';
import {
  AlertTriangle,
  Database,
  HardDrive,
  Info,
  Loader2,
  RefreshCw,
  Settings2,
  Trash2,
} from 'lucide-react';
import { databaseApi } from '@/lib/api';
import { useDatabaseStats } from '@/hooks/useDatabaseStats';
import { useDatabaseMaintenance } from '@/hooks/useDatabaseMaintenance';
import { DiskUsageIndicator } from '@/components/dashboard/DiskUsageIndicator';
import { BackupsSection } from '@/components/settings';
import { useFeedback } from '@/hooks/useFeedback';
import { ConfirmDialog } from '@/components/dialogs';
import type { Task } from 'shared/types';

const DAY_OPTIONS = [
  { value: '1', label: '1 day' },
  { value: '7', label: '7 days' },
  { value: '14', label: '14 days' },
  { value: '30', label: '30 days' },
  { value: '60', label: '60 days' },
  { value: '90', label: '90 days' },
];

function formatBytes(bytes: bigint): string {
  const numBytes = Number(bytes);
  if (numBytes === 0) return '0 B';
  const k = 1024;
  const sizes = ['B', 'KB', 'MB', 'GB'];
  const i = Math.floor(Math.log(numBytes) / Math.log(k));
  return `${parseFloat((numBytes / Math.pow(k, i)).toFixed(1))} ${sizes[i]}`;
}

export function SystemSettings() {
  const { t } = useTranslation('settings');
  const {
    success,
    error: feedbackError,
    showSuccess,
    showError,
  } = useFeedback();

  // Day selectors state
  const [archivedDays, setArchivedDays] = useState('14');
  const [logDays, setLogDays] = useState('14');

  // VACUUM cooldown state (5 minutes)
  const [lastVacuumTime, setLastVacuumTime] = useState<number | null>(null);
  const canVacuum =
    lastVacuumTime === null || Date.now() - lastVacuumTime > 5 * 60 * 1000;

  // Cooldown countdown state
  const [cooldownRemaining, setCooldownRemaining] = useState<number>(0);

  useEffect(() => {
    if (lastVacuumTime === null) {
      setCooldownRemaining(0);
      return;
    }

    const updateRemaining = () => {
      const elapsed = Date.now() - lastVacuumTime;
      const remaining = Math.max(0, 5 * 60 * 1000 - elapsed);
      setCooldownRemaining(remaining);
    };

    updateRemaining();
    const interval = setInterval(updateRemaining, 1000);
    return () => clearInterval(interval);
  }, [lastVacuumTime]);

  const formatCooldown = (ms: number): string => {
    const minutes = Math.floor(ms / 60000);
    const seconds = Math.floor((ms % 60000) / 1000);
    return minutes > 0 ? `${minutes}m ${seconds}s` : `${seconds}s`;
  };

  // Database stats
  const {
    data: stats,
    isLoading: statsLoading,
    error: statsError,
  } = useDatabaseStats();

  // Archived tasks preview
  const {
    data: archivedStats,
    isLoading: archivedStatsLoading,
    refetch: refetchArchivedStats,
  } = useQuery({
    queryKey: ['archivedStats', archivedDays],
    queryFn: () => databaseApi.getArchivedStats(Number(archivedDays)),
    staleTime: 30000,
  });

  // Non-terminal archived tasks (stuck tasks)
  const { data: stuckTasks } = useQuery({
    queryKey: ['archivedNonTerminal'],
    queryFn: () => databaseApi.getArchivedNonTerminal(),
    staleTime: 60000,
  });

  // Log stats preview
  const {
    data: logStats,
    isLoading: logStatsLoading,
    refetch: refetchLogStats,
  } = useQuery({
    queryKey: ['logStats', logDays],
    queryFn: () => databaseApi.getLogStats(Number(logDays)),
    staleTime: 30000,
  });

  // Database maintenance mutations
  const { vacuum, analyze, purgeArchived, purgeLogs } = useDatabaseMaintenance({
    onVacuumSuccess: (result) => {
      setLastVacuumTime(Date.now());
      showSuccess(
        t('settings.system.database.vacuumSuccess', {
          freed: formatBytes(result.freed_bytes),
          defaultValue: `Vacuum complete. Freed ${formatBytes(result.freed_bytes)}.`,
        })
      );
    },
    onVacuumError: () => {
      showError(
        t('settings.system.database.vacuumError', {
          defaultValue: 'Failed to vacuum database.',
        })
      );
    },
    onAnalyzeSuccess: () => {
      showSuccess(
        t('settings.system.database.analyzeSuccess', {
          defaultValue: 'Database analysis complete.',
        })
      );
    },
    onAnalyzeError: () => {
      showError(
        t('settings.system.database.analyzeError', {
          defaultValue: 'Failed to analyze database.',
        })
      );
    },
    onPurgeArchivedSuccess: (result) => {
      showSuccess(
        t('settings.system.cleanup.purgeArchivedSuccess', {
          count: Number(result.deleted),
          defaultValue: `Deleted ${Number(result.deleted)} archived tasks.`,
        })
      );
      refetchArchivedStats();
    },
    onPurgeArchivedError: () => {
      showError(
        t('settings.system.cleanup.purgeArchivedError', {
          defaultValue: 'Failed to purge archived tasks.',
        })
      );
    },
    onPurgeLogsSuccess: (result) => {
      showSuccess(
        t('settings.system.cleanup.purgeLogsSuccess', {
          count: Number(result.deleted),
          defaultValue: `Deleted ${Number(result.deleted)} log entries.`,
        })
      );
      refetchLogStats();
    },
    onPurgeLogsError: () => {
      showError(
        t('settings.system.cleanup.purgeLogsError', {
          defaultValue: 'Failed to purge log entries.',
        })
      );
    },
  });

  const handleOptimize = async () => {
    const result = await ConfirmDialog.show({
      title: t('settings.system.cleanup.confirmVacuumTitle', {
        defaultValue: 'Confirm Database Optimisation',
      }),
      message: t('settings.system.cleanup.confirmVacuumMessage', {
        defaultValue:
          'This will run VACUUM and ANALYSE on the database. The database may be briefly locked during this operation.',
      }),
      confirmText: t('settings.system.database.optimize', {
        defaultValue: 'Optimise Database',
      }),
      cancelText: t('settings.system.cleanup.confirmCancel', {
        defaultValue: 'Cancel',
      }),
      variant: 'info',
    });
    if (result !== 'confirmed') return;

    try {
      await vacuum.mutateAsync();
      await analyze.mutateAsync();
    } catch {
      // Errors handled by mutation callbacks
    }
  };

  const handlePurgeArchived = async () => {
    const count = Number(archivedStats?.count ?? 0);
    const result = await ConfirmDialog.show({
      title: t('settings.system.cleanup.confirmPurgeTitle', {
        defaultValue: 'Confirm Purge',
      }),
      message: t('settings.system.cleanup.confirmPurgeArchived', { count }),
      confirmText: t('settings.system.cleanup.confirmDelete', {
        defaultValue: 'Delete',
      }),
      cancelText: t('settings.system.cleanup.confirmCancel', {
        defaultValue: 'Cancel',
      }),
      variant: 'destructive',
    });
    if (result !== 'confirmed') return;
    purgeArchived.mutate(Number(archivedDays));
  };

  const handlePurgeLogs = async () => {
    const count = Number(logStats?.count ?? 0);
    const result = await ConfirmDialog.show({
      title: t('settings.system.cleanup.confirmPurgeTitle', {
        defaultValue: 'Confirm Purge',
      }),
      message: t('settings.system.cleanup.confirmPurgeLogs', { count }),
      confirmText: t('settings.system.cleanup.confirmDelete', {
        defaultValue: 'Delete',
      }),
      cancelText: t('settings.system.cleanup.confirmCancel', {
        defaultValue: 'Cancel',
      }),
      variant: 'destructive',
    });
    if (result !== 'confirmed') return;
    purgeLogs.mutate(Number(logDays));
  };

  const isOptimizing = vacuum.isPending || analyze.isPending;

  return (
    <div className="space-y-6">
      {feedbackError && (
        <Alert variant="destructive">
          <AlertDescription>{feedbackError}</AlertDescription>
        </Alert>
      )}

      {success && (
        <Alert variant="success">
          <AlertDescription className="font-medium">
            {typeof success === 'string'
              ? success
              : t('settings.system.operationSuccess', {
                  defaultValue: 'Operation completed successfully.',
                })}
          </AlertDescription>
        </Alert>
      )}

      {/* Section 1: Disk Usage */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <HardDrive className="h-5 w-5" />
            {t('settings.system.diskUsage.title', {
              defaultValue: 'Disk Usage',
            })}
          </CardTitle>
          <CardDescription>
            {t('settings.system.diskUsage.description', {
              defaultValue: 'Monitor and manage disk space used by worktrees.',
            })}
          </CardDescription>
        </CardHeader>
        <CardContent>
          <DiskUsageIndicator />
        </CardContent>
      </Card>

      {/* Section 2: Database Maintenance */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <Database className="h-5 w-5" />
            {t('settings.system.database.title', {
              defaultValue: 'Database Maintenance',
            })}
          </CardTitle>
          <CardDescription>
            {t('settings.system.database.description', {
              defaultValue:
                'View database statistics and optimize performance.',
            })}
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <Alert className="mb-4">
            <Info className="h-4 w-4" />
            <AlertDescription>
              {t('settings.system.database.vacuumWarning', {
                defaultValue:
                  'VACUUM may take several minutes on large databases. The database will be briefly locked during this operation.',
              })}
            </AlertDescription>
          </Alert>
          {statsLoading ? (
            <div className="flex items-center gap-2 text-muted-foreground">
              <Loader2 className="h-4 w-4 animate-spin" />
              {t('settings.system.database.loading', {
                defaultValue: 'Loading database statistics...',
              })}
            </div>
          ) : statsError ? (
            <div className="text-destructive">
              {t('settings.system.database.loadError', {
                defaultValue: 'Failed to load database statistics.',
              })}
            </div>
          ) : stats ? (
            <div className="space-y-4">
              {/* Stats display */}
              <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
                <div className="space-y-1">
                  <div className="text-sm text-muted-foreground">
                    {t('settings.system.database.fileSize', {
                      defaultValue: 'Database Size',
                    })}
                  </div>
                  <div className="text-lg font-semibold">
                    {formatBytes(stats.database_size_bytes)}
                  </div>
                </div>
                <div className="space-y-1">
                  <div className="text-sm text-muted-foreground">
                    {t('settings.system.database.walSize', {
                      defaultValue: 'WAL Size',
                    })}
                  </div>
                  <div className="text-lg font-semibold">
                    {formatBytes(stats.wal_size_bytes)}
                  </div>
                </div>
                <div className="space-y-1">
                  <div className="text-sm text-muted-foreground">
                    {t('settings.system.database.pageSize', {
                      defaultValue: 'Page Size',
                    })}
                  </div>
                  <div className="text-lg font-semibold">
                    {formatBytes(stats.page_size)}
                  </div>
                </div>
                <div className="space-y-1">
                  <div className="text-sm text-muted-foreground">
                    {t('settings.system.database.freePages', {
                      defaultValue: 'Free Pages',
                    })}
                  </div>
                  <div className="text-lg font-semibold">
                    {Number(stats.free_pages).toLocaleString()}
                  </div>
                </div>
              </div>

              {/* Table counts */}
              <div className="border rounded-lg p-4">
                <div className="text-sm font-medium mb-2">
                  {t('settings.system.database.tableCounts', {
                    defaultValue: 'Table Row Counts',
                  })}
                </div>
                <div className="grid grid-cols-2 md:grid-cols-4 gap-2 text-sm">
                  <div className="flex justify-between">
                    <span className="text-muted-foreground">Tasks:</span>
                    <span className="font-mono">
                      {Number(stats.task_count).toLocaleString()}
                    </span>
                  </div>
                  <div className="flex justify-between">
                    <span className="text-muted-foreground">Attempts:</span>
                    <span className="font-mono">
                      {Number(stats.task_attempt_count).toLocaleString()}
                    </span>
                  </div>
                  <div className="flex justify-between">
                    <span className="text-muted-foreground">Processes:</span>
                    <span className="font-mono">
                      {Number(stats.execution_process_count).toLocaleString()}
                    </span>
                  </div>
                  <div className="flex justify-between">
                    <span className="text-muted-foreground">Log Entries:</span>
                    <span className="font-mono">
                      {Number(stats.log_entry_count).toLocaleString()}
                    </span>
                  </div>
                </div>
              </div>

              {/* Optimize button */}
              <div className="flex items-center justify-between pt-2">
                <div className="text-sm text-muted-foreground">
                  {t('settings.system.database.optimizeHint', {
                    defaultValue:
                      'Run VACUUM and ANALYZE to reclaim space and optimize queries.',
                  })}
                </div>
                <TooltipProvider>
                  <Tooltip>
                    <TooltipTrigger asChild>
                      <span>
                        <Button
                          onClick={handleOptimize}
                          disabled={isOptimizing || !canVacuum}
                          variant="outline"
                        >
                          {isOptimizing ? (
                            <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                          ) : (
                            <Settings2 className="mr-2 h-4 w-4" />
                          )}
                          {t('settings.system.database.optimize', {
                            defaultValue: 'Optimize Database',
                          })}
                        </Button>
                      </span>
                    </TooltipTrigger>
                    {!canVacuum && cooldownRemaining > 0 && (
                      <TooltipContent>
                        {t('settings.system.cleanup.vacuumCooldownTooltip', {
                          remaining: formatCooldown(cooldownRemaining),
                        })}
                      </TooltipContent>
                    )}
                  </Tooltip>
                </TooltipProvider>
              </div>
            </div>
          ) : null}
        </CardContent>
      </Card>

      {/* Section 3: Data Cleanup */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <Trash2 className="h-5 w-5" />
            {t('settings.system.cleanup.title', {
              defaultValue: 'Data Cleanup',
            })}
          </CardTitle>
          <CardDescription>
            {t('settings.system.cleanup.description', {
              defaultValue:
                'Remove old archived tasks and log entries to free up space.',
            })}
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-6">
          {/* Stuck tasks warning */}
          {stuckTasks && stuckTasks.length > 0 && (
            <Alert variant="destructive">
              <AlertTriangle className="h-4 w-4" />
              <AlertDescription>
                {t('settings.system.cleanup.stuckTasksWarning', {
                  count: stuckTasks.length,
                  defaultValue: `Found ${stuckTasks.length} archived task(s) that are not in a terminal state (done/cancelled). These may need attention.`,
                })}
                <div className="mt-2 flex flex-wrap gap-1">
                  {stuckTasks.slice(0, 5).map((task: Task) => (
                    <Badge key={task.id} variant="outline" className="text-xs">
                      {task.title.substring(0, 30)}
                      {task.title.length > 30 ? '...' : ''}
                    </Badge>
                  ))}
                  {stuckTasks.length > 5 && (
                    <Badge variant="outline" className="text-xs">
                      +{stuckTasks.length - 5} more
                    </Badge>
                  )}
                </div>
              </AlertDescription>
            </Alert>
          )}

          {/* Archived tasks cleanup */}
          <div className="border rounded-lg p-4 space-y-4">
            <div className="flex items-start justify-between">
              <div>
                <div className="font-medium">
                  {t('settings.system.cleanup.archivedTasks.title', {
                    defaultValue: 'Archived Tasks',
                  })}
                </div>
                <div className="text-sm text-muted-foreground">
                  {t('settings.system.cleanup.archivedTasks.description', {
                    defaultValue:
                      'Delete completed/cancelled archived tasks older than the selected period.',
                  })}
                </div>
              </div>
            </div>

            <div className="flex items-center gap-4">
              <div className="flex items-center gap-2">
                <Label
                  htmlFor="archived-days"
                  className="text-sm whitespace-nowrap"
                >
                  {t('settings.system.cleanup.olderThan', {
                    defaultValue: 'Older than',
                  })}
                </Label>
                <Select value={archivedDays} onValueChange={setArchivedDays}>
                  <SelectTrigger id="archived-days" className="w-[120px]">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {DAY_OPTIONS.map((opt) => (
                      <SelectItem key={opt.value} value={opt.value}>
                        {opt.label}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>

              <div className="flex items-center gap-2">
                {archivedStatsLoading ? (
                  <Loader2 className="h-4 w-4 animate-spin text-muted-foreground" />
                ) : (
                  <Badge variant="secondary">
                    {Number(archivedStats?.count ?? 0)}{' '}
                    {t('settings.system.cleanup.tasksFound', {
                      defaultValue: 'tasks found',
                    })}
                  </Badge>
                )}
                <Button
                  variant="ghost"
                  size="icon"
                  className="h-8 w-8"
                  onClick={() => refetchArchivedStats()}
                >
                  <RefreshCw className="h-4 w-4" />
                </Button>
              </div>

              <Button
                variant="destructive"
                onClick={handlePurgeArchived}
                disabled={
                  purgeArchived.isPending ||
                  archivedStatsLoading ||
                  !archivedStats?.count
                }
                className="ml-auto"
              >
                {purgeArchived.isPending ? (
                  <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                ) : (
                  <Trash2 className="mr-2 h-4 w-4" />
                )}
                {t('settings.system.cleanup.purge', {
                  defaultValue: 'Purge Tasks',
                })}
              </Button>
            </div>
          </div>

          {/* Log entries cleanup */}
          <div className="border rounded-lg p-4 space-y-4">
            <div className="flex items-start justify-between">
              <div>
                <div className="font-medium">
                  {t('settings.system.cleanup.logEntries.title', {
                    defaultValue: 'Log Entries',
                  })}
                </div>
                <div className="text-sm text-muted-foreground">
                  {t('settings.system.cleanup.logEntries.description', {
                    defaultValue:
                      'Delete log entries older than the selected period.',
                  })}
                </div>
              </div>
            </div>

            <div className="flex items-center gap-4">
              <div className="flex items-center gap-2">
                <Label htmlFor="log-days" className="text-sm whitespace-nowrap">
                  {t('settings.system.cleanup.olderThan', {
                    defaultValue: 'Older than',
                  })}
                </Label>
                <Select value={logDays} onValueChange={setLogDays}>
                  <SelectTrigger id="log-days" className="w-[120px]">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {DAY_OPTIONS.map((opt) => (
                      <SelectItem key={opt.value} value={opt.value}>
                        {opt.label}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>

              <div className="flex items-center gap-2">
                {logStatsLoading ? (
                  <Loader2 className="h-4 w-4 animate-spin text-muted-foreground" />
                ) : (
                  <Badge variant="secondary">
                    {Number(logStats?.count ?? 0)}{' '}
                    {t('settings.system.cleanup.entriesFound', {
                      defaultValue: 'entries found',
                    })}
                  </Badge>
                )}
                <Button
                  variant="ghost"
                  size="icon"
                  className="h-8 w-8"
                  onClick={() => refetchLogStats()}
                >
                  <RefreshCw className="h-4 w-4" />
                </Button>
              </div>

              <Button
                variant="destructive"
                onClick={handlePurgeLogs}
                disabled={
                  purgeLogs.isPending || logStatsLoading || !logStats?.count
                }
                className="ml-auto"
              >
                {purgeLogs.isPending ? (
                  <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                ) : (
                  <Trash2 className="mr-2 h-4 w-4" />
                )}
                {t('settings.system.cleanup.purge', {
                  defaultValue: 'Purge Logs',
                })}
              </Button>
            </div>
          </div>
        </CardContent>
      </Card>

      {/* Section 4: Backups */}
      <BackupsSection onSuccess={showSuccess} onError={showError} />
    </div>
  );
}
