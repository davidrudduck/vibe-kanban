import { useQuery } from '@tanstack/react-query';
import { projectsApi } from '@/lib/api';
import type { UnifiedProjectsResponse } from 'shared/types';

/**
 * Hook to fetch unified projects from all nodes in the organization.
 *
 * Returns local projects first, then remote projects grouped by node.
 * Remote projects that are already linked to a local project are excluded.
 *
 * @deprecated Use useMergedProjects instead. This hook will be removed in a future release.
 * The unified-projects endpoint is being replaced by merged-projects for a consistent
 * project view across the application.
 */
export function useUnifiedProjects() {
  return useQuery<UnifiedProjectsResponse>({
    queryKey: ['unified-projects'],
    queryFn: () => projectsApi.getUnified(),
    staleTime: 30000, // Consider data fresh for 30 seconds
  });
}
