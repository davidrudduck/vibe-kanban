import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import {
  cleanArtifacts,
  getDiagnostics,
  getDiskUsage,
  removeWorktree,
} from '@/shared/lib/diagnostics';

export const DIAGNOSTICS_QUERY_KEY = ['diagnostics'] as const;
export const DISK_USAGE_QUERY_KEY = ['diagnostics', 'disk-usage'] as const;

export function useDiagnostics() {
  return useQuery({
    queryKey: DIAGNOSTICS_QUERY_KEY,
    queryFn: getDiagnostics,
    refetchInterval: 10_000,
  });
}

export function useDiskUsage() {
  return useQuery({
    queryKey: DISK_USAGE_QUERY_KEY,
    queryFn: getDiskUsage,
    refetchInterval: 60_000,
  });
}

export function useCleanArtifacts() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (workspaceId: string) => cleanArtifacts(workspaceId),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: DISK_USAGE_QUERY_KEY });
    },
  });
}

export function useRemoveWorktree() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (workspaceId: string) => removeWorktree(workspaceId),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: DISK_USAGE_QUERY_KEY });
    },
  });
}
