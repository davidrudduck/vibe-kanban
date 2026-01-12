import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { AlertTriangle, Loader2, CheckCircle2, Wrench } from 'lucide-react';
import { Button } from '@/components/ui/button';
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from '@/components/ui/card';
import { useSwarmHealth } from '@/hooks/useSwarmHealth';
import { useProjectMutations } from '@/hooks/useProjectMutations';
import type { UnlinkSwarmRequest } from 'shared/types';

export function SwarmHealthSection() {
  const { t } = useTranslation(['settings', 'common']);
  const swarmHealth = useSwarmHealth();
  const { unlinkFromSwarm } = useProjectMutations();
  const [isFixing, setIsFixing] = useState(false);

  // Hide when no issues or loading
  if (swarmHealth.isLoading || swarmHealth.isHealthy) {
    return null;
  }

  const handleFixAll = async () => {
    if (swarmHealth.projectsWithIssues === 0) return;

    const confirmed = window.confirm(
      t(
        'settings.swarm.health.fixAllConfirm',
        'This will unlink {{count}} project(s) from the swarm, clearing all sync state. Are you sure?',
        { count: swarmHealth.projectsWithIssues }
      )
    );
    if (!confirmed) return;

    setIsFixing(true);
    let successCount = 0;
    let errorCount = 0;

    try {
      // Get projects with issues by fetching all projects and their sync health
      const projectsResponse = await fetch('/api/projects');
      if (!projectsResponse.ok) {
        throw new Error('Failed to fetch projects');
      }
      const projectsResult = await projectsResponse.json();
      const projects = projectsResult.data || [];

      // Find projects with issues
      for (const project of projects) {
        if (!project.local_project_id) continue;

        const healthResponse = await fetch(
          `/api/projects/${project.local_project_id}/sync-health`
        );
        if (!healthResponse.ok) continue;

        const healthResult = await healthResponse.json();
        if (healthResult.data?.has_sync_issues) {
          try {
            const unlinkRequest: UnlinkSwarmRequest = { notify_hive: false };
            await unlinkFromSwarm.mutateAsync({
              projectId: project.local_project_id,
              data: unlinkRequest,
            });
            successCount++;
          } catch (err) {
            console.error(`Failed to unlink project ${project.name}:`, err);
            errorCount++;
          }
        }
      }

      if (errorCount > 0) {
        alert(
          t(
            'settings.swarm.health.partialSuccess',
            'Fixed {{success}} project(s), {{error}} failed. Check console for details.',
            { success: successCount, error: errorCount }
          )
        );
      } else {
        alert(
          t(
            'settings.swarm.health.success',
            'Successfully fixed {{count}} project(s).',
            { count: successCount }
          )
        );
      }
    } catch (error) {
      console.error('Failed to fix swarm issues:', error);
      alert(
        t(
          'settings.swarm.health.error',
          'Failed to fix swarm issues. Please check console for details.'
        )
      );
    } finally {
      setIsFixing(false);
    }
  };

  return (
    <Card>
      <CardHeader>
        <div className="flex items-center gap-2">
          <AlertTriangle className="h-5 w-5 text-amber-500" />
          <CardTitle className="text-lg">
            {t('settings.swarm.health.title', 'Swarm Health Issues')}
          </CardTitle>
        </div>
        <CardDescription>
          {t(
            'settings.swarm.health.description',
            'Some projects have sync state issues that need to be resolved.'
          )}
        </CardDescription>
      </CardHeader>

      <CardContent className="space-y-4">
        {/* Summary stats */}
        <div className="flex items-center gap-6 text-sm">
          <div className="flex items-center gap-2">
            <AlertTriangle className="h-4 w-4 text-amber-500" />
            <span>
              {t(
                'settings.swarm.health.projectsWithIssues',
                '{{count}} project(s) with issues',
                { count: swarmHealth.projectsWithIssues }
              )}
            </span>
          </div>
          <div className="flex items-center gap-2">
            <Wrench className="h-4 w-4 text-muted-foreground" />
            <span>
              {t(
                'settings.swarm.health.orphanedTasks',
                '{{count}} orphaned task(s)',
                { count: swarmHealth.totalOrphanedTasks }
              )}
            </span>
          </div>
        </div>

        {/* Fix All button */}
        <div className="flex items-center gap-2">
          <Button
            onClick={handleFixAll}
            disabled={isFixing}
            size="sm"
            className="gap-2"
          >
            {isFixing ? (
              <>
                <Loader2 className="h-4 w-4 animate-spin" />
                <span>
                  {t('settings.swarm.health.fixing', 'Fixing...')}
                </span>
              </>
            ) : (
              <>
                <Wrench className="h-4 w-4" />
                <span>
                  {t('settings.swarm.health.fixAll', 'Fix All Issues')}
                </span>
              </>
            )}
          </Button>
          <p className="text-sm text-muted-foreground">
            {t(
              'settings.swarm.health.fixAllHint',
              'This will unlink projects from the swarm and clear all sync state.'
            )}
          </p>
        </div>

        {/* Success indicator after fixing */}
        {swarmHealth.isHealthy && !swarmHealth.isLoading && (
          <div className="flex items-center gap-2 text-sm text-green-600">
            <CheckCircle2 className="h-4 w-4" />
            <span>
              {t(
                'settings.swarm.health.resolved',
                'All sync issues resolved!'
              )}
            </span>
          </div>
        )}
      </CardContent>
    </Card>
  );
}
