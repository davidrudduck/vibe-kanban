import { useQuery } from '@tanstack/react-query';
import { projectsApi } from '@/lib/api';
import type { SyncHealthResponse } from 'shared/types';

export function useProjectSyncHealth(projectId?: string) {
  return useQuery<SyncHealthResponse, Error>({
    queryKey: ['project', projectId, 'sync-health'],
    queryFn: () => projectsApi.getSyncHealth(projectId!),
    enabled: Boolean(projectId),
    staleTime: 5 * 60 * 1000, // Cache for 5 minutes
    refetchOnWindowFocus: true,
  });
}
