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

function formatBytes(bytes: bigint | number): string {
  const n = typeof bytes === 'bigint' ? Number(bytes) : bytes;
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  if (n < 1024 * 1024 * 1024) return `${(n / (1024 * 1024)).toFixed(1)} MB`;
  return `${(n / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

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

function ConfirmDialog({
  message,
  onConfirm,
  onCancel,
}: {
  message: string;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
      <div className="bg-panel border border-border rounded-sm p-6 max-w-sm w-full mx-4 space-y-4">
        <div className="flex items-start gap-3">
          <WarningIcon
            className="size-5 text-warning shrink-0 mt-0.5"
            weight="bold"
          />
          <p className="text-sm text-normal">{message}</p>
        </div>
        <div className="flex justify-end gap-2">
          <PrimaryButton variant="tertiary" value="Cancel" onClick={onCancel} />
          <PrimaryButton
            variant="secondary"
            value="Confirm"
            onClick={onConfirm}
          />
        </div>
      </div>
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
  const [showArchivedConfirm, setShowArchivedConfirm] = useState(false);
  const archivedStats = useArchivedStats(
    showArchivedStats ? Number(archivedDays) : undefined
  );
  const purgeArchivedMutation = usePurgeArchived();

  // Log cleanup state
  const [logDays, setLogDays] = useState<string>('14');
  const [showLogStats, setShowLogStats] = useState(false);
  const [showLogConfirm, setShowLogConfirm] = useState(false);
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

        {analyzeMutation.isSuccess && (
          <p className="text-sm text-success flex items-center gap-1.5">
            <CheckCircleIcon className="size-icon-sm" weight="bold" />
            ANALYZE complete
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
            onClick={() => setShowArchivedConfirm(true)}
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
            onClick={() => setShowLogConfirm(true)}
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
      </SettingsCard>

      {/* Confirm dialogs */}
      {showArchivedConfirm && (
        <ConfirmDialog
          message={`This will permanently delete archived workspaces older than ${archivedDays} days. This cannot be undone.`}
          onConfirm={() => {
            setShowArchivedConfirm(false);
            purgeArchivedMutation.mutate(Number(archivedDays));
          }}
          onCancel={() => setShowArchivedConfirm(false)}
        />
      )}

      {showLogConfirm && (
        <ConfirmDialog
          message={`This will permanently delete log files older than ${logDays} days. This cannot be undone.`}
          onConfirm={() => {
            setShowLogConfirm(false);
            purgeLogsMutation.mutate(Number(logDays));
          }}
          onCancel={() => setShowLogConfirm(false)}
        />
      )}
    </>
  );
}
