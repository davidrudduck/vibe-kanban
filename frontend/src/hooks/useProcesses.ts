import { useQuery } from '@tanstack/react-query';
import { processesApi } from '@/lib/api';
import type { ProcessInfo, ProcessFilter } from 'shared/types';

export interface UseProcessesResult {
  processes: ProcessInfo[];
  isLoading: boolean;
  error: Error | null;
  refetch: () => void;
}

/**
 * Hook to fetch and poll vibe-kanban related processes with optional filtering.
 * Supports filtering by project, task, task attempt, or executors only.
 */
export const useProcesses = (filter?: ProcessFilter): UseProcessesResult => {
  const { data, isLoading, error, refetch } = useQuery({
    queryKey: ['processes', filter],
    queryFn: () => processesApi.list(filter),
    // Only poll when tab is visible to reduce unnecessary network requests
    refetchInterval: () => (document.hidden ? false : 5000),
    staleTime: 2000,
  });

  return {
    processes: data ?? [],
    isLoading,
    error: error as Error | null,
    refetch,
  };
};
