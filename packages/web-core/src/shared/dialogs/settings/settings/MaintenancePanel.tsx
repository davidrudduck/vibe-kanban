import { useState } from 'react';
import { useNavigate } from '@tanstack/react-router';
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
  useArchivedList,
  useLogStats,
  usePurgeLogs,
  useLogList,
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
  const navigate = useNavigate();

  const {
    data: stats,
    isLoading: statsLoading,
    isError: statsIsError,
    error: statsError,
    refetch: refetchStats,
  } = useDatabaseStats();

  const vacuumMutation = useRunVacuum();
  const analyzeMutation = useRunAnalyze();

  // Archived cleanup state
  const [archivedDays, setArchivedDays] = useState<string>('14');
  const [showArchivedStats, setShowArchivedStats] = useState(false);
  const [showArchivedList, setShowArchivedList] = useState(false);
  const archivedStats = useArchivedStats(
    showArchivedStats ? Number(archivedDays) : undefined
  );
  const archivedList = useArchivedList(
    showArchivedList ? Number(archivedDays) : undefined
  );
  const purgeArchivedMutation = usePurgeArchived();

  // Log cleanup state
  const [logDays, setLogDays] = useState<string>('14');
  const [showLogStats, setShowLogStats] = useState(false);
  const [showLogList, setShowLogList] = useState(false);
  const logStats = useLogStats(showLogStats ? Number(logDays) : undefined);
  const logList = useLogList(showLogList ? Number(logDays) : undefined);
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
        {statsIsError && (
          <p className="text-sm text-error py-2">
            Failed to load database stats:{' '}
            {(statsError as Error)?.message ?? 'Unknown error'}
          </p>
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
              setShowArchivedList(false);
              purgeArchivedMutation.reset();
            }}
          />
        </SettingsField>

        <div className="flex flex-wrap gap-2">
          <PrimaryButton
            variant="tertiary"
            onClick={() => {
              setShowArchivedStats(true);
              setShowArchivedList(true);
              archivedStats.refetch();
              archivedList.refetch();
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

        {showArchivedList && archivedList.data && archivedList.data.items.length > 0 && (
          <div className="rounded-sm border border-border overflow-hidden mt-2">
            <div className="flex items-center justify-between px-3 py-1.5 bg-secondary/50 border-b border-border">
              <span className="text-xs font-medium text-low uppercase tracking-wide">
                Workspace
              </span>
              <span className="text-xs font-medium text-low uppercase tracking-wide">
                Archived
              </span>
            </div>
            {archivedList.data.items.map((item) => (
              <div
                key={item.id}
                className="flex items-center justify-between px-3 py-1.5 border-b border-border last:border-b-0"
              >
                <button
                  className="text-sm text-link hover:underline truncate max-w-[60%] text-left"
                  title={item.name ?? 'Unnamed workspace'}
                  onClick={() =>
                    navigate({
                      to: '/workspaces/$workspaceId',
                      params: { workspaceId: item.id },
                    })
                  }
                >
                  {item.name ?? 'Unnamed workspace'}
                </button>
                <span className="text-xs font-mono text-low shrink-0">
                  {new Date(item.archived_at).toLocaleDateString()}
                </span>
              </div>
            ))}
            <div className="px-3 py-1.5 bg-secondary/50 border-t border-border text-xs text-low">
              {archivedList.data.items.length} workspace(s) eligible (oldest
              first)
            </div>
          </div>
        )}

        {showArchivedList && archivedList.data?.items.length === 0 && (
          <p className="text-sm text-low mt-1">
            No archived workspaces older than {archivedDays} days.
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
              setShowLogList(false);
              purgeLogsMutation.reset();
            }}
          />
        </SettingsField>

        <div className="flex flex-wrap gap-2">
          <PrimaryButton
            variant="tertiary"
            onClick={() => {
              setShowLogStats(true);
              setShowLogList(true);
              logStats.refetch();
              logList.refetch();
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

        {showLogList && logList.data && logList.data.items.length > 0 && (
          <div className="rounded-sm border border-border overflow-hidden mt-2">
            <div className="flex items-center justify-between px-3 py-1.5 bg-secondary/50 border-b border-border">
              <span className="text-xs font-medium text-low uppercase tracking-wide">
                Workspace
              </span>
              <span className="text-xs font-medium text-low uppercase tracking-wide">
                Files / Size / Oldest
              </span>
            </div>
            {logList.data.items.map((item) => (
              <div
                key={item.session_id}
                className="flex items-center justify-between px-3 py-1.5 border-b border-border last:border-b-0"
              >
                <button
                  className="text-sm text-link hover:underline truncate max-w-[50%] text-left"
                  title={item.workspace_name ?? 'Unnamed workspace'}
                  onClick={() =>
                    navigate({
                      to: '/workspaces/$workspaceId',
                      params: { workspaceId: item.workspace_id },
                    })
                  }
                >
                  {item.workspace_name ?? 'Unnamed workspace'}
                </button>
                <span className="text-xs font-mono text-low shrink-0">
                  {String(item.file_count)} / {formatBytes(item.total_bytes)} /{' '}
                  {item.oldest_file_date}
                </span>
              </div>
            ))}
            <div className="px-3 py-1.5 bg-secondary/50 border-t border-border text-xs text-low">
              {logList.data.items.length} session(s) with eligible log files
              (oldest first)
            </div>
          </div>
        )}

        {showLogList && logList.data?.items.length === 0 && (
          <p className="text-sm text-low mt-1">
            No log files older than {logDays} days.
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
