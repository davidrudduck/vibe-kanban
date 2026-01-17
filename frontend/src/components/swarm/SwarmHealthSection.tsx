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
import {
  AlertDialog,
  AlertDialogContent,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogDescription,
  AlertDialogFooter,
} from '@/components/ui/alert-dialog';
import { useSwarmHealth } from '@/hooks/useSwarmHealth';
import {
  useSwarmHealthActions,
  type FixAllResult,
} from '@/hooks/useSwarmHealthActions';

export function SwarmHealthSection() {
  const { t } = useTranslation(['settings', 'common']);
  const swarmHealth = useSwarmHealth();
  const [showConfirmDialog, setShowConfirmDialog] = useState(false);
  const [showResultDialog, setShowResultDialog] = useState(false);
  const [result, setResult] = useState<FixAllResult | null>(null);

  const { fixAllIssues, isFixing } = useSwarmHealthActions({
    onFixAllSuccess: (res) => {
      setResult(res);
      setShowResultDialog(true);
    },
    onFixAllPartial: (res) => {
      setResult(res);
      setShowResultDialog(true);
    },
    onFixAllError: (error) => {
      console.error('Failed to fix all issues:', error);
      setResult({
        successCount: 0,
        errorCount: 1,
        errors: [{ projectId: '', projectName: 'Unknown', error }],
      });
      setShowResultDialog(true);
    },
  });

  // Hide when no issues or loading
  if (swarmHealth.isLoading || swarmHealth.isHealthy) {
    return null;
  }

  const handleFixAllClick = () => {
    if (swarmHealth.projectsWithIssues === 0) return;
    setShowConfirmDialog(true);
  };

  const handleConfirmedFix = async () => {
    setShowConfirmDialog(false);
    await fixAllIssues();
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
            onClick={handleFixAllClick}
            disabled={isFixing}
            size="sm"
            className="gap-2"
          >
            {isFixing ? (
              <>
                <Loader2 className="h-4 w-4 animate-spin" />
                <span>{t('settings.swarm.health.fixing', 'Fixing...')}</span>
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
              {t('settings.swarm.health.resolved', 'All sync issues resolved!')}
            </span>
          </div>
        )}
      </CardContent>

      {/* Confirmation Dialog */}
      <AlertDialog open={showConfirmDialog} onOpenChange={setShowConfirmDialog}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>
              {t(
                'settings.swarm.health.fixAll.confirmTitle',
                'Fix All Swarm Issues'
              )}
            </AlertDialogTitle>
            <AlertDialogDescription>
              {t(
                'settings.swarm.health.fixAll.confirmDescription',
                'This will unlink {{count}} project(s) from the swarm to fix sync issues. This action cannot be undone.',
                { count: swarmHealth.projectsWithIssues }
              )}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <Button
              variant="outline"
              onClick={() => setShowConfirmDialog(false)}
              disabled={isFixing}
            >
              {t('common:cancel', 'Cancel')}
            </Button>
            <Button
              variant="destructive"
              onClick={handleConfirmedFix}
              disabled={isFixing}
            >
              {isFixing ? (
                <>
                  <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                  {t('settings.swarm.health.fixAll.fixing', 'Fixing...')}
                </>
              ) : (
                t('settings.swarm.health.fixAll.confirm', 'Fix All')
              )}
            </Button>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>

      {/* Result Dialog */}
      <AlertDialog open={showResultDialog} onOpenChange={setShowResultDialog}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>
              {result?.errorCount === 0
                ? t('settings.swarm.health.fixAll.successTitle', 'Issues Fixed')
                : t(
                    'settings.swarm.health.fixAll.partialTitle',
                    'Partial Success'
                  )}
            </AlertDialogTitle>
            <AlertDialogDescription>
              {result?.errorCount === 0 ? (
                t(
                  'settings.swarm.health.fixAll.successMessage',
                  'Successfully fixed {{count}} project(s).',
                  { count: result?.successCount }
                )
              ) : (
                <div className="space-y-2">
                  <p>
                    {t(
                      'settings.swarm.health.fixAll.partialMessage',
                      'Fixed {{success}} project(s), {{failed}} failed.',
                      {
                        success: result?.successCount,
                        failed: result?.errorCount,
                      }
                    )}
                  </p>
                  {result?.errors && result.errors.length > 0 && (
                    <div className="mt-2">
                      <p className="font-medium text-sm">
                        {t(
                          'settings.swarm.health.fixAll.failedProjects',
                          'Failed projects:'
                        )}
                      </p>
                      <ul className="mt-1 text-sm list-disc list-inside">
                        {result.errors.map((err, i) => (
                          <li key={i}>{err.projectName}</li>
                        ))}
                      </ul>
                    </div>
                  )}
                </div>
              )}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <Button onClick={() => setShowResultDialog(false)}>
              {t('common:ok', 'OK')}
            </Button>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </Card>
  );
}
