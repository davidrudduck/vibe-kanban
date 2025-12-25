import { useState, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import { Button } from '@/components/ui/button';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { AlertTriangle, RefreshCw, Activity, XCircle } from 'lucide-react';
import { useProcesses } from '@/hooks/useProcesses';
import { useProcessMutations } from '@/hooks/useProcessMutations';
import { ProcessList } from '@/components/processes';
import type { ProcessFilter, Project } from 'shared/types';
import { useProjects } from '@/hooks/useProjects';

type FilterMode = 'all' | 'executors_only' | 'by_project';

export function Processes() {
  const { t } = useTranslation(['processes', 'common']);

  // Filter state
  const [filterMode, setFilterMode] = useState<FilterMode>('all');
  const [selectedProjectId, setSelectedProjectId] = useState<string | null>(
    null
  );
  const [killingPids, setKillingPids] = useState<Set<number>>(new Set());

  // Build filter based on state
  const filter: ProcessFilter | undefined =
    filterMode === 'executors_only'
      ? {
          executors_only: true,
          project_id: null,
          task_id: null,
          task_attempt_id: null,
        }
      : filterMode === 'by_project' && selectedProjectId
        ? {
            project_id: selectedProjectId,
            executors_only: false,
            task_id: null,
            task_attempt_id: null,
          }
        : undefined;

  const { processes, isLoading, error, refetch } = useProcesses(filter);
  const { data: projectsData } = useProjects();
  const projects: Project[] = projectsData ?? [];

  const { killProcesses } = useProcessMutations({
    onKillSuccess: (result) => {
      setKillingPids(new Set());
      if (result.killed_count > 0) {
        console.log(`Successfully killed ${result.killed_count} process(es)`);
      }
      if (result.failed_count > 0) {
        console.error(`Failed to kill ${result.failed_count} process(es)`);
      }
    },
    onKillError: (err) => {
      setKillingPids(new Set());
      console.error('Kill error:', err);
    },
  });

  const handleKillProcess = useCallback(
    (pid: number) => {
      const confirmed = window.confirm(
        t('killDialog.singleDescription', { pid })
      );
      if (!confirmed) return;

      setKillingPids(new Set([pid]));
      killProcesses.mutate({ scope: { type: 'single', pid }, force: false });
    },
    [killProcesses, t]
  );

  const handleKillAll = useCallback(() => {
    const confirmed = window.confirm(
      t('killDialog.allDescription', { count: processes.length })
    );
    if (!confirmed) return;

    setKillingPids(new Set(processes.map((p) => p.pid)));
    killProcesses.mutate({ scope: { type: 'all' }, force: false });
  }, [killProcesses, processes, t]);

  const handleFilterModeChange = (value: string) => {
    setFilterMode(value as FilterMode);
    if (value !== 'by_project') {
      setSelectedProjectId(null);
    }
  };

  if (error) {
    return (
      <div className="p-8">
        <Alert variant="destructive">
          <AlertTriangle className="h-4 w-4" />
          <AlertTitle>{t('common:states.error')}</AlertTitle>
          <AlertDescription>
            {error.message || t('error.loadFailed')}
          </AlertDescription>
        </Alert>
      </div>
    );
  }

  return (
    <div className="space-y-6 p-8 pb-16 md:pb-8 h-full overflow-auto">
      {/* Header */}
      <div className="flex flex-col gap-4 sm:flex-row sm:justify-between sm:items-start">
        <div>
          <h1 className="text-2xl sm:text-3xl font-bold tracking-tight flex items-center gap-2">
            <Activity className="h-6 w-6 sm:h-8 sm:w-8" />
            {t('title')}
          </h1>
          <p className="text-muted-foreground text-sm sm:text-base">
            {t('description')}
          </p>
        </div>
        <div className="flex flex-wrap gap-2">
          <Button
            variant="outline"
            size="sm"
            onClick={() => refetch()}
            disabled={isLoading}
          >
            <RefreshCw className="mr-2 h-4 w-4" />
            {t('actions.refresh')}
          </Button>
          {processes.length > 0 && (
            <Button
              variant="destructive"
              size="sm"
              onClick={handleKillAll}
              disabled={killProcesses.isPending}
            >
              <XCircle className="mr-2 h-4 w-4" />
              {t('actions.killAll')}
            </Button>
          )}
        </div>
      </div>

      {/* Filters */}
      <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:gap-4">
        <Select value={filterMode} onValueChange={handleFilterModeChange}>
          <SelectTrigger className="w-full sm:w-[200px]">
            <SelectValue placeholder={t('filters.mode')} />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="all">{t('filters.all')}</SelectItem>
            <SelectItem value="executors_only">
              {t('filters.executorsOnly')}
            </SelectItem>
            <SelectItem value="by_project">{t('filters.byProject')}</SelectItem>
          </SelectContent>
        </Select>

        {filterMode === 'by_project' && (
          <Select
            value={selectedProjectId ?? ''}
            onValueChange={(value) => setSelectedProjectId(value || null)}
          >
            <SelectTrigger className="w-full sm:w-[250px]">
              <SelectValue placeholder={t('filters.selectProject')} />
            </SelectTrigger>
            <SelectContent>
              {projects.map((project) => (
                <SelectItem key={project.id} value={project.id}>
                  {project.name}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        )}

        <span className="text-sm text-muted-foreground">
          {t('processCount', { count: processes.length })}
        </span>
      </div>

      {/* Process List */}
      <ProcessList
        processes={processes}
        isLoading={isLoading}
        onKillProcess={handleKillProcess}
        killingPids={killingPids}
      />
    </div>
  );
}
