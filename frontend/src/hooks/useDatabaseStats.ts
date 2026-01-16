import { useQuery } from '@tanstack/react-query';
import { databaseApi } from '@/lib/api';

/**
 * Hook to fetch database statistics.
 *
 * Returns information about database file sizes, page counts,
 * and table row counts for monitoring database health.
 */
export function useDatabaseStats() {
  return useQuery({
    queryKey: ['databaseStats'],
    queryFn: () => databaseApi.getStats(),
    // Refetch every 60 seconds
    refetchInterval: 60000,
    // Stale after 30 seconds
    staleTime: 30000,
  });
}
