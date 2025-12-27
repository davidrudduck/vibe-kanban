import { useMutation, useQueryClient } from '@tanstack/react-query';
import { nodesApi } from '@/lib/api';
import type { NodeApiKey, MergeNodesResponse } from '@/types/nodes';

interface UseNodeMutationsOptions {
  onDeleteSuccess?: () => void;
  onDeleteError?: (err: unknown) => void;
  onMergeSuccess?: (result: MergeNodesResponse) => void;
  onMergeError?: (err: unknown) => void;
  onUnblockKeySuccess?: (key: NodeApiKey) => void;
  onUnblockKeyError?: (err: unknown) => void;
}

export function useNodeMutations(options?: UseNodeMutationsOptions) {
  const queryClient = useQueryClient();

  const deleteNode = useMutation({
    mutationKey: ['deleteNode'],
    mutationFn: (nodeId: string) => nodesApi.delete(nodeId),
    onSuccess: (_data, nodeId) => {
      // Remove from cache
      queryClient.removeQueries({ queryKey: ['node', nodeId] });
      queryClient.removeQueries({ queryKey: ['node', nodeId, 'projects'] });

      // Invalidate nodes list (will refetch with node removed)
      queryClient.invalidateQueries({ queryKey: ['nodes'] });

      options?.onDeleteSuccess?.();
    },
    onError: (err) => {
      console.error('Failed to delete node:', err);
      options?.onDeleteError?.(err);
    },
  });

  const mergeNodes = useMutation({
    mutationKey: ['mergeNodes'],
    mutationFn: ({
      sourceNodeId,
      targetNodeId,
    }: {
      sourceNodeId: string;
      targetNodeId: string;
    }) => nodesApi.mergeNodes(sourceNodeId, targetNodeId),
    onSuccess: (result: MergeNodesResponse) => {
      // Remove source node from cache (it was deleted)
      queryClient.removeQueries({ queryKey: ['node', result.source_node_id] });
      queryClient.removeQueries({
        queryKey: ['node', result.source_node_id, 'projects'],
      });

      // Invalidate target node's projects (projects were moved to it)
      queryClient.invalidateQueries({
        queryKey: ['node', result.target_node_id, 'projects'],
      });

      // Invalidate nodes list
      queryClient.invalidateQueries({ queryKey: ['nodes'] });

      // Invalidate API keys (keys may have been rebound)
      queryClient.invalidateQueries({ queryKey: ['apiKeys'] });

      options?.onMergeSuccess?.(result);
    },
    onError: (err) => {
      console.error('Failed to merge nodes:', err);
      options?.onMergeError?.(err);
    },
  });

  const unblockApiKey = useMutation({
    mutationKey: ['unblockApiKey'],
    mutationFn: (keyId: string) => nodesApi.unblockApiKey(keyId),
    onSuccess: (key: NodeApiKey) => {
      // Invalidate API keys list to reflect the unblocked state
      queryClient.invalidateQueries({ queryKey: ['apiKeys'] });

      options?.onUnblockKeySuccess?.(key);
    },
    onError: (err) => {
      console.error('Failed to unblock API key:', err);
      options?.onUnblockKeyError?.(err);
    },
  });

  return {
    deleteNode,
    mergeNodes,
    unblockApiKey,
  };
}
