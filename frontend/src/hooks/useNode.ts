import { useQuery } from '@tanstack/react-query';
import { nodesApi } from '@/lib/api';
import type { Node } from '@/types/nodes';

/**
 * Hook to fetch a single node by ID.
 */
export function useNode(
  nodeId: string | undefined,
  options?: { enabled?: boolean }
) {
  return useQuery<Node>({
    queryKey: ['node', nodeId],
    queryFn: () => nodesApi.getById(nodeId!),
    enabled: options?.enabled !== false && !!nodeId,
    staleTime: 30000, // 30 seconds
  });
}
