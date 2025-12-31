import { useQuery } from '@tanstack/react-query';
import { projectsApi } from '@/lib/api';
import type { MergedProjectsResponse } from 'shared/types';

/**
 * Hook to fetch merged projects (local and remote combined).
 *
 * Returns a unified list of projects where projects with the same
 * swarm_project_id are merged into a single entry showing all locations.
 */
export function useMergedProjects() {
  return useQuery<MergedProjectsResponse>({
    queryKey: ['merged-projects'],
    queryFn: () => projectsApi.getMerged(),
    staleTime: 30000, // Consider data fresh for 30 seconds
  });
}
