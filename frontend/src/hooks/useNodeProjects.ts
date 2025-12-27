import { useQuery } from '@tanstack/react-query';
import { nodesApi } from '@/lib/api';
import type { NodeProject } from '@/types/nodes';

/**
 * Hook to fetch projects registered on a specific node.
 */
export function useNodeProjects(
  nodeId: string | undefined,
  options?: { enabled?: boolean }
) {
  return useQuery<NodeProject[]>({
    queryKey: ['node', nodeId, 'projects'],
    queryFn: () => nodesApi.listProjects(nodeId!),
    enabled: options?.enabled !== false && !!nodeId,
    staleTime: 30000, // 30 seconds
  });
}
