import { useState } from 'react';
import {
  ArrowClockwiseIcon,
  DatabaseIcon,
  TrashIcon,
  SpinnerIcon,
  CheckCircleIcon,
  WarningIcon,
} from '@phosphor-icons/react';
import { PrimaryButton } from '@vibe/ui/components/PrimaryButton';
import { ConfirmDialog } from '@vibe/ui/components/ConfirmDialog';
import { formatBytes } from '@/shared/lib/utils';
import {
  SettingsCard,
  SettingsField,
  SettingsSelect,
} from './SettingsComponents';
import {
  useDatabaseStats,
  useRunVacuum,
  useRunAnalyze,
  useArchivedStats,
  usePurgeArchived,
  useLogStats,
  usePurgeLogs,
} from '@/shared/hooks/useDatabaseMaintenance';

const DAYS_OPTIONS: { value: string; label: string }[] = [
  { value: '7', label: '7 days' },
  { value: '14', label: '14 days' },
  { value: '30', label: '30 days' },
  { value: '60', label: '60 days' },
  { value: '90', label: '90 days' },
];

function StatRow({
  label,
  value,
}: {
  label: string;
  value: string | number | bigint;
}) {
  return (
    <div className="flex items-center justify-between py-1.5 border-b border-border last:border-b-0">
      <span className="text-sm text-low">{label}</span>
      <span className="text-sm font-mono text-normal">{String(value)}</span>
    </div>
  );
}

export function MaintenancePanel() {
  const {
    data: stats,
    isLoading: statsLoading,
    refetch: refetchStats,
  } = useDatabaseStats();

  const vacuumMutation = useRunVacuum();
  const analyzeMutation = useRunAnalyze();

  // Archived cleanup state
  const [archivedDays, setArchivedDays] = useState<string>('14');
  const [showArchivedStats, setShowArchivedStats] = useState(false);
  const archivedStats = useArchivedStats(
    showArchivedStats ? Number(archivedDays) : undefined
  );
  const purgeArchivedMutation = usePurgeArchived();

  // Log cleanup state
  const [logDays, setLogDays] = useState<string>('14');
  const [showLogStats, setShowLogStats] = useState(false);
  const logStats = useLogStats(showLogStats ? Number(logDays) : undefined);
  const purgeLogsMutation = usePurgeLogs();

  const isVacuumCooldown =
    vacuumMutation.error != null &&
    (vacuumMutation.error as { status?: number }).status === 429;

  return (
    <>
      {/* Database Stats */}
      <SettingsCard
        title="Database Stats"
        description="Current size and row counts for the local database."
        headerAction={
          <PrimaryButton
            variant="tertiary"
            onClick={() => refetchStats()}
            disabled={statsLoading}
          >
            <ArrowClockwiseIcon
              className={`size-icon-sm ${statsLoading ? 'animate-spin' : ''}`}
              weight="bold"
            />
            Refresh
          </PrimaryButton>
        }
      >
        {statsLoading && (
          <div className="flex items-center gap-2 text-sm text-low py-2">
            <SpinnerIcon className="size-icon-sm animate-spin" weight="bold" />
            Loading...
          </div>
        )}
        {stats && (
          <div className="rounded-sm border border-border overflow-hidden">
            <StatRow
              label="Database size"
              value={formatBytes(stats.database_size_bytes)}
            />
            <StatRow
              label="WAL size"
              value={formatBytes(stats.wal_size_bytes)}
            />
            <StatRow label="Free pages" value={String(stats.free_pages)} />
            <StatRow label="Total pages" value={String(stats.page_count)} />
            <StatRow label="Tasks" value={String(stats.task_count)} />
            <StatRow label="Workspaces" value={String(stats.workspace_count)} />
            <StatRow
              label="Execution processes"
              value={String(stats.execution_process_count)}
            />
          </div>
        )}
      </SettingsCard>

      {/* Vacuum & Analyze */}
      <SettingsCard
        title="Vacuum & Analyze"
        description="Reclaim unused space and update query planner statistics."
      >
        <p className="text-sm text-low">
          VACUUM rebuilds the database file to reclaim free pages. It requires
          brief exclusive access to the database during the operation.
        </p>

        <div className="flex flex-wrap gap-2">
          <PrimaryButton
            variant="tertiary"
            onClick={() => vacuumMutation.mutate()}
            disabled={vacuumMutation.isPending || isVacuumCooldown}
          >
            {vacuumMutation.isPending ? (
              <SpinnerIcon
                className="size-icon-sm animate-spin"
                weight="bold"
              />
            ) : (
              <DatabaseIcon className="size-icon-sm" weight="bold" />
            )}
            VACUUM
          </PrimaryButton>

          <PrimaryButton
            variant="tertiary"
            onClick={() => analyzeMutation.mutate()}
            disabled={analyzeMutation.isPending}
          >
            {analyzeMutation.isPending ? (
              <SpinnerIcon
                className="size-icon-sm animate-spin"
                weight="bold"
              />
            ) : (
              <DatabaseIcon className="size-icon-sm" weight="bold" />
            )}
            ANALYZE
          </PrimaryButton>
        </div>

        {isVacuumCooldown && (
          <p className="text-sm text-warning flex items-center gap-1.5">
            <WarningIcon className="size-icon-sm" weight="bold" />
            VACUUM is on cooldown. Please wait before running again.
          </p>
        )}

        {vacuumMutation.isSuccess && vacuumMutation.data && (
          <p className="text-sm text-success flex items-center gap-1.5">
            <CheckCircleIcon className="size-icon-sm" weight="bold" />
            VACUUM complete — freed{' '}
            {formatBytes(vacuumMutation.data.bytes_freed)}
          </p>
        )}

        {vacuumMutation.isError && !isVacuumCooldown && (
          <p className="text-sm text-error mt-2">
            Error: {(vacuumMutation.error as Error).message}
          </p>
        )}

        {analyzeMutation.isSuccess && (
          <p className="text-sm text-success flex items-center gap-1.5">
            <CheckCircleIcon className="size-icon-sm" weight="bold" />
            ANALYZE complete
          </p>
        )}

        {analyzeMutation.isError && (
          <p className="text-sm text-error mt-2">
            Error: {(analyzeMutation.error as Error).message}
          </p>
        )}
      </SettingsCard>

      {/* Archived Workspace Cleanup */}
      <SettingsCard
        title="Archived Workspace Cleanup"
        description="Remove old archived workspaces to free up disk space."
      >
        <SettingsField label="Older than">
          <SettingsSelect
            value={archivedDays}
            options={DAYS_OPTIONS}
            onChange={(value) => {
              setArchivedDays(value);
              setShowArchivedStats(false);
              purgeArchivedMutation.reset();
            }}
          />
        </SettingsField>

        <div className="flex flex-wrap gap-2">
          <PrimaryButton
            variant="tertiary"
            onClick={() => {
              setShowArchivedStats(true);
              archivedStats.refetch();
            }}
            disabled={archivedStats.isFetching}
          >
            {archivedStats.isFetching ? (
              <SpinnerIcon
                className="size-icon-sm animate-spin"
                weight="bold"
              />
            ) : (
              <ArrowClockwiseIcon className="size-icon-sm" weight="bold" />
            )}
            Check
          </PrimaryButton>

          <PrimaryButton
            variant="secondary"
            onClick={async () => {
              const result = await ConfirmDialog.show({
                title: 'Purge Archived Workspaces',
                message: `This will permanently delete archived workspaces older than ${archivedDays} days. This cannot be undone.`,
                confirmText: 'Purge',
                variant: 'destructive',
              });
              if (result === 'confirmed') {
                purgeArchivedMutation.mutate(Number(archivedDays));
              }
            }}
            disabled={purgeArchivedMutation.isPending}
          >
            {purgeArchivedMutation.isPending ? (
              <SpinnerIcon
                className="size-icon-sm animate-spin"
                weight="bold"
              />
            ) : (
              <TrashIcon className="size-icon-sm" weight="bold" />
            )}
            Purge
          </PrimaryButton>
        </div>

        {showArchivedStats && archivedStats.data && (
          <p className="text-sm text-normal">
            {String(archivedStats.data.count)} workspace(s) eligible for removal
            (older than {String(archivedStats.data.older_than_days)} days)
          </p>
        )}

        {purgeArchivedMutation.isSuccess && purgeArchivedMutation.data && (
          <p className="text-sm text-success flex items-center gap-1.5">
            <CheckCircleIcon className="size-icon-sm" weight="bold" />
            Deleted {String(purgeArchivedMutation.data.deleted)} workspace(s),
            skipped {String(purgeArchivedMutation.data.skipped_active)} active
          </p>
        )}

        {purgeArchivedMutation.isError && (
          <p className="text-sm text-error mt-2">
            Error: {(purgeArchivedMutation.error as Error).message}
          </p>
        )}
      </SettingsCard>

      {/* Log File Cleanup */}
      <SettingsCard
        title="Log File Cleanup"
        description="Remove old log files to free up disk space."
      >
        <SettingsField label="Older than">
          <SettingsSelect
            value={logDays}
            options={DAYS_OPTIONS}
            onChange={(value) => {
              setLogDays(value);
              setShowLogStats(false);
              purgeLogsMutation.reset();
            }}
          />
        </SettingsField>

        <div className="flex flex-wrap gap-2">
          <PrimaryButton
            variant="tertiary"
            onClick={() => {
              setShowLogStats(true);
              logStats.refetch();
            }}
            disabled={logStats.isFetching}
          >
            {logStats.isFetching ? (
              <SpinnerIcon
                className="size-icon-sm animate-spin"
                weight="bold"
              />
            ) : (
              <ArrowClockwiseIcon className="size-icon-sm" weight="bold" />
            )}
            Check
          </PrimaryButton>

          <PrimaryButton
            variant="secondary"
            onClick={async () => {
              const result = await ConfirmDialog.show({
                title: 'Purge Log Files',
                message: `This will permanently delete log files older than ${logDays} days. This cannot be undone.`,
                confirmText: 'Purge',
                variant: 'destructive',
              });
              if (result === 'confirmed') {
                purgeLogsMutation.mutate(Number(logDays));
              }
            }}
            disabled={purgeLogsMutation.isPending}
          >
            {purgeLogsMutation.isPending ? (
              <SpinnerIcon
                className="size-icon-sm animate-spin"
                weight="bold"
              />
            ) : (
              <TrashIcon className="size-icon-sm" weight="bold" />
            )}
            Purge
          </PrimaryButton>
        </div>

        {showLogStats && logStats.data && (
          <p className="text-sm text-normal">
            {String(logStats.data.file_count)} file(s),{' '}
            {formatBytes(logStats.data.total_bytes)} total (older than{' '}
            {String(logStats.data.older_than_days)} days)
          </p>
        )}

        {purgeLogsMutation.isSuccess && purgeLogsMutation.data && (
          <p className="text-sm text-success flex items-center gap-1.5">
            <CheckCircleIcon className="size-icon-sm" weight="bold" />
            Deleted {String(purgeLogsMutation.data.deleted_files)} file(s),
            freed {formatBytes(purgeLogsMutation.data.bytes_freed)}
          </p>
        )}

        {purgeLogsMutation.isError && (
          <p className="text-sm text-error mt-2">
            Error: {(purgeLogsMutation.error as Error).message}
          </p>
        )}
      </SettingsCard>
    </>
  );
}
