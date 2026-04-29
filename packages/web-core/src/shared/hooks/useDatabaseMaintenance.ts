import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import {
  getDatabaseStats,
  runVacuum,
  runAnalyze,
  getArchivedStats,
  purgeArchived,
  getLogStats,
  purgeLogs,
} from '@/shared/lib/database';

export const DATABASE_STATS_QUERY_KEY = ['database', 'stats'] as const;

export function useDatabaseStats() {
  return useQuery({
    queryKey: DATABASE_STATS_QUERY_KEY,
    queryFn: getDatabaseStats,
    refetchInterval: 30_000,
  });
}

export function useRunVacuum() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: runVacuum,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: DATABASE_STATS_QUERY_KEY });
    },
  });
}

export function useRunAnalyze() {
  return useMutation({
    mutationFn: runAnalyze,
  });
}

export function useArchivedStats(olderThanDays?: number) {
  return useQuery({
    queryKey: ['database', 'archived-stats', olderThanDays] as const,
    queryFn: () => getArchivedStats(olderThanDays!),
    enabled: olderThanDays != null,
  });
}

export function usePurgeArchived() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (olderThanDays?: number) => purgeArchived(olderThanDays),
    onSuccess: () => {
      queryClient.invalidateQueries({
        queryKey: ['database', 'archived-stats'],
      });
      queryClient.invalidateQueries({ queryKey: DATABASE_STATS_QUERY_KEY });
    },
  });
}

export function useLogStats(olderThanDays?: number) {
  return useQuery({
    queryKey: ['database', 'log-stats', olderThanDays] as const,
    queryFn: () => getLogStats(olderThanDays!),
    enabled: olderThanDays != null,
  });
}

export function usePurgeLogs() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (olderThanDays?: number) => purgeLogs(olderThanDays),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['database', 'log-stats'] });
      queryClient.invalidateQueries({ queryKey: DATABASE_STATS_QUERY_KEY });
    },
  });
}
