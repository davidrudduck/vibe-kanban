import { useQuery, useQueries } from '@tanstack/react-query';
import { projectsApi } from '@/lib/api';
import type { Project } from 'shared/types';

export interface SwarmHealthSummary {
  totalProjects: number;
  projectsWithIssues: number;
  totalOrphanedTasks: number;
  isHealthy: boolean;
  isLoading: boolean;
  error: Error | null;
}

export function useSwarmHealth(): SwarmHealthSummary {
  // Fetch all projects
  const projectsQuery = useQuery<Project[]>({
    queryKey: ['projects'],
    queryFn: () => projectsApi.getAll(),
    staleTime: 30000, // Consider data fresh for 30 seconds
  });

  // Fetch sync health for each project in parallel
  const syncHealthQueries = useQueries({
    queries:
      projectsQuery.data?.map((project) => ({
        queryKey: ['project', project.id, 'sync-health'],
        queryFn: () => projectsApi.getSyncHealth(project.id),
        enabled: projectsQuery.isSuccess && !!project.id,
        staleTime: 5 * 60 * 1000, // Cache for 5 minutes
        refetchOnWindowFocus: true,
      })) || [],
  });

  const isLoading = projectsQuery.isLoading || syncHealthQueries.some((q) => q.isLoading);
  const hasError = projectsQuery.error || syncHealthQueries.some((q) => q.error);
  const error = (projectsQuery.error as Error | null) || (syncHealthQueries.find((q) => q.error)?.error as Error | null);

  // Aggregate results
  let projectsWithIssues = 0;
  let totalOrphanedTasks = 0;

  if (!isLoading && !hasError) {
    for (const query of syncHealthQueries) {
      if (query.data?.has_sync_issues) {
        projectsWithIssues++;
      }
      if (query.data?.orphaned_task_count) {
        totalOrphanedTasks += Number(query.data.orphaned_task_count);
      }
    }
  }

  const isHealthy = !isLoading && !hasError && projectsWithIssues === 0;

  return {
    totalProjects: projectsQuery.data?.length || 0,
    projectsWithIssues,
    totalOrphanedTasks,
    isHealthy,
    isLoading,
    error,
  };
}
