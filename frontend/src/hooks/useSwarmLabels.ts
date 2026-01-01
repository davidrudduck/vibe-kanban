import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { swarmLabelsApi } from '@/lib/api';
import type {
  SwarmLabel,
  CreateSwarmLabelRequest,
  UpdateSwarmLabelRequest,
} from '@/types/swarm';

// Query keys for cache management
export const swarmLabelsKeys = {
  all: ['swarmLabels'] as const,
  lists: () => [...swarmLabelsKeys.all, 'list'] as const,
  list: (organizationId: string) =>
    [...swarmLabelsKeys.lists(), organizationId] as const,
  details: () => [...swarmLabelsKeys.all, 'detail'] as const,
  detail: (id: string) => [...swarmLabelsKeys.details(), id] as const,
};

interface UseSwarmLabelsOptions {
  organizationId: string;
  enabled?: boolean;
}

/**
 * Hook to fetch and cache swarm labels for an organization.
 */
export function useSwarmLabels({
  organizationId,
  enabled = true,
}: UseSwarmLabelsOptions) {
  return useQuery({
    queryKey: swarmLabelsKeys.list(organizationId),
    queryFn: () => swarmLabelsApi.list(organizationId),
    enabled: enabled && !!organizationId,
    staleTime: 30_000, // Consider data stale after 30 seconds
  });
}

interface UseSwarmLabelOptions {
  labelId: string;
  enabled?: boolean;
}

/**
 * Hook to fetch a single swarm label by ID.
 */
export function useSwarmLabel({
  labelId,
  enabled = true,
}: UseSwarmLabelOptions) {
  return useQuery({
    queryKey: swarmLabelsKeys.detail(labelId),
    queryFn: () => swarmLabelsApi.getById(labelId),
    enabled: enabled && !!labelId,
  });
}

interface UseSwarmLabelMutationsOptions {
  organizationId: string;
  onCreateSuccess?: (label: SwarmLabel) => void;
  onCreateError?: (err: unknown) => void;
  onUpdateSuccess?: (label: SwarmLabel) => void;
  onUpdateError?: (err: unknown) => void;
  onDeleteSuccess?: () => void;
  onDeleteError?: (err: unknown) => void;
  onMergeSuccess?: (label: SwarmLabel) => void;
  onMergeError?: (err: unknown) => void;
  onPromoteSuccess?: (label: SwarmLabel) => void;
  onPromoteError?: (err: unknown) => void;
}

/**
 * Hook providing mutations for swarm label CRUD operations.
 */
export function useSwarmLabelMutations(options: UseSwarmLabelMutationsOptions) {
  const queryClient = useQueryClient();
  const { organizationId } = options;

  const invalidateLabels = () => {
    queryClient.invalidateQueries({
      queryKey: swarmLabelsKeys.list(organizationId),
    });
  };

  const createLabel = useMutation({
    mutationKey: ['createSwarmLabel'],
    mutationFn: (data: CreateSwarmLabelRequest) => swarmLabelsApi.create(data),
    onSuccess: (label: SwarmLabel) => {
      queryClient.setQueryData(swarmLabelsKeys.detail(label.id), label);
      invalidateLabels();
      options.onCreateSuccess?.(label);
    },
    onError: (err) => {
      console.error('Failed to create swarm label:', err);
      options.onCreateError?.(err);
    },
  });

  const updateLabel = useMutation({
    mutationKey: ['updateSwarmLabel'],
    mutationFn: ({
      labelId,
      data,
    }: {
      labelId: string;
      data: UpdateSwarmLabelRequest;
    }) => swarmLabelsApi.update(labelId, data),
    onSuccess: (label: SwarmLabel) => {
      queryClient.setQueryData(swarmLabelsKeys.detail(label.id), label);
      // Update in list cache
      queryClient.setQueryData<SwarmLabel[]>(
        swarmLabelsKeys.list(organizationId),
        (old) => {
          if (!old) return old;
          return old.map((l) => (l.id === label.id ? label : l));
        }
      );
      options.onUpdateSuccess?.(label);
    },
    onError: (err) => {
      console.error('Failed to update swarm label:', err);
      options.onUpdateError?.(err);
    },
  });

  const deleteLabel = useMutation({
    mutationKey: ['deleteSwarmLabel'],
    mutationFn: (labelId: string) => swarmLabelsApi.delete(labelId),
    onSuccess: (_, labelId) => {
      queryClient.removeQueries({
        queryKey: swarmLabelsKeys.detail(labelId),
      });
      invalidateLabels();
      options.onDeleteSuccess?.();
    },
    onError: (err) => {
      console.error('Failed to delete swarm label:', err);
      options.onDeleteError?.(err);
    },
  });

  const mergeLabels = useMutation({
    mutationKey: ['mergeSwarmLabels'],
    mutationFn: ({
      targetId,
      sourceId,
    }: {
      targetId: string;
      sourceId: string;
    }) => swarmLabelsApi.merge(targetId, { source_id: sourceId }),
    onSuccess: (label: SwarmLabel, { sourceId }) => {
      // Remove the source label from cache
      queryClient.removeQueries({
        queryKey: swarmLabelsKeys.detail(sourceId),
      });
      queryClient.setQueryData(swarmLabelsKeys.detail(label.id), label);
      invalidateLabels();
      options.onMergeSuccess?.(label);
    },
    onError: (err) => {
      console.error('Failed to merge swarm labels:', err);
      options.onMergeError?.(err);
    },
  });

  const promoteToOrg = useMutation({
    mutationKey: ['promoteSwarmLabel'],
    mutationFn: (labelId: string) =>
      swarmLabelsApi.promoteToOrg({ label_id: labelId }),
    onSuccess: (label: SwarmLabel) => {
      queryClient.setQueryData(swarmLabelsKeys.detail(label.id), label);
      invalidateLabels();
      options.onPromoteSuccess?.(label);
    },
    onError: (err) => {
      console.error('Failed to promote label to org:', err);
      options.onPromoteError?.(err);
    },
  });

  return {
    createLabel,
    updateLabel,
    deleteLabel,
    mergeLabels,
    promoteToOrg,
  };
}
