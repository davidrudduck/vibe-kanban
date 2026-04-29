import { useState } from 'react';
import { ArrowClockwiseIcon, SpinnerIcon } from '@phosphor-icons/react';
import { PrimaryButton } from '@vibe/ui/components/PrimaryButton';
import { SettingsCard } from './SettingsComponents';
import { useDiagnostics } from '@/shared/hooks/useDiagnostics';
import { getDiskUsage } from '@/shared/lib/diagnostics';
import type { DiskUsageResponse } from 'shared/types';

function formatBytes(bytes: bigint | number): string {
  const n = typeof bytes === 'bigint' ? Number(bytes) : bytes;
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  if (n < 1024 * 1024 * 1024) return `${(n / (1024 * 1024)).toFixed(1)} MB`;
  return `${(n / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

function WalStatusDot({ walSizeBytes }: { walSizeBytes: bigint }) {
  const bytes = Number(walSizeBytes);
  const MB = 1024 * 1024;
  if (bytes < 50 * MB) {
    return (
      <span className="inline-flex items-center gap-1.5 text-sm text-success">
        <span className="size-2 rounded-full bg-success inline-block" />
        Healthy
      </span>
    );
  }
  if (bytes < 100 * MB) {
    return (
      <span className="inline-flex items-center gap-1.5 text-sm text-warning">
        <span className="size-2 rounded-full bg-warning inline-block" />
        Elevated
      </span>
    );
  }
  return (
    <span className="inline-flex items-center gap-1.5 text-sm text-error">
      <span className="size-2 rounded-full bg-error inline-block" />
      High
    </span>
  );
}

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

export function DiagnosticsPanel() {
  const { data: diagnostics, isLoading: diagLoading } = useDiagnostics();

  const [diskUsage, setDiskUsage] = useState<DiskUsageResponse | null>(null);
  const [diskLoading, setDiskLoading] = useState(false);
  const [diskError, setDiskError] = useState<string | null>(null);

  const handleRefreshDisk = async () => {
    setDiskLoading(true);
    setDiskError(null);
    try {
      const data = await getDiskUsage();
      setDiskUsage(data);
    } catch (err) {
      setDiskError(
        err instanceof Error ? err.message : 'Failed to fetch disk usage'
      );
    } finally {
      setDiskLoading(false);
    }
  };

  return (
    <>
      {/* Connection Pool */}
      <SettingsCard
        title="Connection Pool"
        description="SQLite connection pool statistics, auto-refreshes every 10 seconds."
      >
        {diagLoading && (
          <div className="flex items-center gap-2 text-sm text-low py-2">
            <SpinnerIcon className="size-icon-sm animate-spin" weight="bold" />
            Loading...
          </div>
        )}
        {diagnostics && (
          <div className="rounded-sm border border-border overflow-hidden">
            <StatRow label="Pool size" value={diagnostics.pool_stats.size} />
            <StatRow
              label="Idle connections"
              value={diagnostics.pool_stats.idle}
            />
            <StatRow
              label="Acquired connections"
              value={diagnostics.pool_stats.acquired}
            />
          </div>
        )}
      </SettingsCard>

      {/* WAL Status */}
      <SettingsCard
        title="WAL Status"
        description="Write-Ahead Log size indicator. Large WAL files may indicate write pressure."
      >
        {diagLoading && (
          <div className="flex items-center gap-2 text-sm text-low py-2">
            <SpinnerIcon className="size-icon-sm animate-spin" weight="bold" />
            Loading...
          </div>
        )}
        {diagnostics && (
          <div className="space-y-2">
            <div className="flex items-center justify-between">
              <span className="text-sm text-low">WAL size</span>
              <span className="text-sm font-mono text-normal">
                {diagnostics.wal_size_human}
              </span>
            </div>
            <div className="flex items-center justify-between">
              <span className="text-sm text-low">Status</span>
              <WalStatusDot walSizeBytes={diagnostics.wal_size_bytes} />
            </div>
          </div>
        )}
      </SettingsCard>

      {/* Disk Usage */}
      <SettingsCard
        title="Disk Usage"
        description="Per-workspace disk usage breakdown."
        headerAction={
          <PrimaryButton
            variant="tertiary"
            onClick={handleRefreshDisk}
            disabled={diskLoading}
          >
            {diskLoading ? (
              <SpinnerIcon
                className="size-icon-sm animate-spin"
                weight="bold"
              />
            ) : (
              <ArrowClockwiseIcon className="size-icon-sm" weight="bold" />
            )}
            Refresh
          </PrimaryButton>
        }
      >
        {diskError && <p className="text-sm text-error">{diskError}</p>}

        {!diskUsage && !diskLoading && (
          <p className="text-sm text-low">
            Click Refresh to load workspace disk usage.
          </p>
        )}

        {diskUsage && (
          <>
            <div className="rounded-sm border border-border overflow-hidden">
              <div className="flex items-center justify-between px-3 py-1.5 bg-secondary/50 border-b border-border">
                <span className="text-xs font-medium text-low uppercase tracking-wide">
                  Path
                </span>
                <span className="text-xs font-medium text-low uppercase tracking-wide">
                  Size
                </span>
              </div>
              {diskUsage.workspaces.length === 0 && (
                <div className="px-3 py-3 text-sm text-low text-center">
                  No workspace data available.
                </div>
              )}
              {diskUsage.workspaces.map((ws) => (
                <div
                  key={ws.workspace_id}
                  className="flex items-center justify-between px-3 py-1.5 border-b border-border last:border-b-0"
                >
                  <span
                    className="text-sm text-normal truncate max-w-[65%]"
                    title={ws.path}
                  >
                    {ws.path}
                  </span>
                  <span className="text-sm font-mono text-normal shrink-0">
                    {formatBytes(ws.size_bytes)}
                  </span>
                </div>
              ))}
              <div className="flex items-center justify-between px-3 py-1.5 bg-secondary/50 border-t border-border">
                <span className="text-sm font-medium text-normal">Total</span>
                <span className="text-sm font-mono font-medium text-normal">
                  {diskUsage.total_human}
                </span>
              </div>
            </div>
          </>
        )}
      </SettingsCard>
    </>
  );
}
