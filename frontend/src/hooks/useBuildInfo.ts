import { useQuery } from '@tanstack/react-query';
import { healthApi, HealthResponse } from '@/lib/api';

export interface BuildInfo {
  version: string;
  gitCommit: string;
  gitBranch: string;
  buildTimestamp: string;
}

export function useBuildInfo() {
  const { data, isLoading, error } = useQuery<HealthResponse>({
    queryKey: ['health'],
    queryFn: healthApi.check,
    staleTime: 5 * 60 * 1000, // Cache for 5 minutes
    refetchOnWindowFocus: false,
  });

  const buildInfo: BuildInfo | null = data
    ? {
        version: data.version,
        gitCommit: data.git_commit,
        gitBranch: data.git_branch,
        buildTimestamp: data.build_timestamp,
      }
    : null;

  return {
    buildInfo,
    isLoading,
    error,
  };
}
