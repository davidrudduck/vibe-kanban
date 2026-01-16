import { useMutation, useQueryClient } from '@tanstack/react-query';
import { databaseApi } from '@/lib/api';
import type {
  VacuumResult,
  ArchivedPurgeResult,
  LogPurgeResult,
} from 'shared/types';

interface UseDatabaseMaintenanceOptions {
  onVacuumSuccess?: (result: VacuumResult) => void;
  onVacuumError?: (err: unknown) => void;
  onAnalyzeSuccess?: () => void;
  onAnalyzeError?: (err: unknown) => void;
  onPurgeArchivedSuccess?: (result: ArchivedPurgeResult) => void;
  onPurgeArchivedError?: (err: unknown) => void;
  onPurgeLogsSuccess?: (result: LogPurgeResult) => void;
  onPurgeLogsError?: (err: unknown) => void;
}

/**
 * Provides mutation hooks for database maintenance operations.
 *
 * - vacuum: Reclaims space from deleted records
 * - analyze: Updates query planner statistics
 * - purgeArchived: Deletes old archived terminal tasks
 * - purgeLogs: Deletes old log entries
 */
export function useDatabaseMaintenance(
  options?: UseDatabaseMaintenanceOptions
) {
  const queryClient = useQueryClient();

  const vacuum = useMutation({
    mutationKey: ['databaseVacuum'],
    mutationFn: () => databaseApi.vacuum(),
    onSuccess: (result: VacuumResult) => {
      queryClient.invalidateQueries({ queryKey: ['databaseStats'] });
      options?.onVacuumSuccess?.(result);
    },
    onError: (err) => {
      console.error('Failed to vacuum database:', err);
      options?.onVacuumError?.(err);
    },
  });

  const analyze = useMutation({
    mutationKey: ['databaseAnalyze'],
    mutationFn: () => databaseApi.analyze(),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['databaseStats'] });
      options?.onAnalyzeSuccess?.();
    },
    onError: (err) => {
      console.error('Failed to analyze database:', err);
      options?.onAnalyzeError?.(err);
    },
  });

  const purgeArchived = useMutation({
    mutationKey: ['purgeArchivedTasks'],
    mutationFn: (olderThanDays?: number) =>
      databaseApi.purgeArchived(olderThanDays),
    onSuccess: (result: ArchivedPurgeResult) => {
      queryClient.invalidateQueries({ queryKey: ['databaseStats'] });
      queryClient.invalidateQueries({ queryKey: ['tasks'] });
      options?.onPurgeArchivedSuccess?.(result);
    },
    onError: (err) => {
      console.error('Failed to purge archived tasks:', err);
      options?.onPurgeArchivedError?.(err);
    },
  });

  const purgeLogs = useMutation({
    mutationKey: ['purgeLogs'],
    mutationFn: (olderThanDays?: number) =>
      databaseApi.purgeLogs(olderThanDays),
    onSuccess: (result: LogPurgeResult) => {
      queryClient.invalidateQueries({ queryKey: ['databaseStats'] });
      options?.onPurgeLogsSuccess?.(result);
    },
    onError: (err) => {
      console.error('Failed to purge log entries:', err);
      options?.onPurgeLogsError?.(err);
    },
  });

  return { vacuum, analyze, purgeArchived, purgeLogs };
}
