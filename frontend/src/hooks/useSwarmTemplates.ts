import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { swarmTemplatesApi } from '@/lib/api';
import type {
  SwarmTemplate,
  CreateSwarmTemplateRequest,
  UpdateSwarmTemplateRequest,
} from '@/types/swarm';

// Query keys for cache management
export const swarmTemplatesKeys = {
  all: ['swarmTemplates'] as const,
  lists: () => [...swarmTemplatesKeys.all, 'list'] as const,
  list: (organizationId: string) =>
    [...swarmTemplatesKeys.lists(), organizationId] as const,
  details: () => [...swarmTemplatesKeys.all, 'detail'] as const,
  detail: (id: string) => [...swarmTemplatesKeys.details(), id] as const,
};

interface UseSwarmTemplatesOptions {
  organizationId: string;
  enabled?: boolean;
}

/**
 * Hook to fetch and cache swarm templates for an organization.
 */
export function useSwarmTemplates({
  organizationId,
  enabled = true,
}: UseSwarmTemplatesOptions) {
  return useQuery({
    queryKey: swarmTemplatesKeys.list(organizationId),
    queryFn: () => swarmTemplatesApi.list(organizationId),
    enabled: enabled && !!organizationId,
    staleTime: 30_000, // Consider data stale after 30 seconds
  });
}

interface UseSwarmTemplateOptions {
  templateId: string;
  enabled?: boolean;
}

/**
 * Hook to fetch a single swarm template by ID.
 */
export function useSwarmTemplate({
  templateId,
  enabled = true,
}: UseSwarmTemplateOptions) {
  return useQuery({
    queryKey: swarmTemplatesKeys.detail(templateId),
    queryFn: () => swarmTemplatesApi.getById(templateId),
    enabled: enabled && !!templateId,
  });
}

interface UseSwarmTemplateMutationsOptions {
  organizationId: string;
  onCreateSuccess?: (template: SwarmTemplate) => void;
  onCreateError?: (err: unknown) => void;
  onUpdateSuccess?: (template: SwarmTemplate) => void;
  onUpdateError?: (err: unknown) => void;
  onDeleteSuccess?: () => void;
  onDeleteError?: (err: unknown) => void;
  onMergeSuccess?: (template: SwarmTemplate) => void;
  onMergeError?: (err: unknown) => void;
}

/**
 * Hook providing mutations for swarm template CRUD operations.
 */
export function useSwarmTemplateMutations(
  options: UseSwarmTemplateMutationsOptions
) {
  const queryClient = useQueryClient();
  const { organizationId } = options;

  const invalidateTemplates = () => {
    queryClient.invalidateQueries({
      queryKey: swarmTemplatesKeys.list(organizationId),
    });
  };

  const createTemplate = useMutation({
    mutationKey: ['createSwarmTemplate'],
    mutationFn: (data: CreateSwarmTemplateRequest) =>
      swarmTemplatesApi.create(data),
    onSuccess: (template: SwarmTemplate) => {
      queryClient.setQueryData(
        swarmTemplatesKeys.detail(template.id),
        template
      );
      invalidateTemplates();
      options.onCreateSuccess?.(template);
    },
    onError: (err) => {
      console.error('Failed to create swarm template:', err);
      options.onCreateError?.(err);
    },
  });

  const updateTemplate = useMutation({
    mutationKey: ['updateSwarmTemplate'],
    mutationFn: ({
      templateId,
      data,
    }: {
      templateId: string;
      data: UpdateSwarmTemplateRequest;
    }) => swarmTemplatesApi.update(templateId, data),
    onSuccess: (template: SwarmTemplate) => {
      queryClient.setQueryData(
        swarmTemplatesKeys.detail(template.id),
        template
      );
      // Update in list cache
      queryClient.setQueryData<SwarmTemplate[]>(
        swarmTemplatesKeys.list(organizationId),
        (old) => {
          if (!old) return old;
          return old.map((t) => (t.id === template.id ? template : t));
        }
      );
      options.onUpdateSuccess?.(template);
    },
    onError: (err) => {
      console.error('Failed to update swarm template:', err);
      options.onUpdateError?.(err);
    },
  });

  const deleteTemplate = useMutation({
    mutationKey: ['deleteSwarmTemplate'],
    mutationFn: (templateId: string) => swarmTemplatesApi.delete(templateId),
    onSuccess: (_, templateId) => {
      queryClient.removeQueries({
        queryKey: swarmTemplatesKeys.detail(templateId),
      });
      invalidateTemplates();
      options.onDeleteSuccess?.();
    },
    onError: (err) => {
      console.error('Failed to delete swarm template:', err);
      options.onDeleteError?.(err);
    },
  });

  const mergeTemplates = useMutation({
    mutationKey: ['mergeSwarmTemplates'],
    mutationFn: ({
      targetId,
      sourceId,
    }: {
      targetId: string;
      sourceId: string;
    }) => swarmTemplatesApi.merge(targetId, { source_id: sourceId }),
    onSuccess: (template: SwarmTemplate, { sourceId }) => {
      // Remove the source template from cache
      queryClient.removeQueries({
        queryKey: swarmTemplatesKeys.detail(sourceId),
      });
      queryClient.setQueryData(
        swarmTemplatesKeys.detail(template.id),
        template
      );
      invalidateTemplates();
      options.onMergeSuccess?.(template);
    },
    onError: (err) => {
      console.error('Failed to merge swarm templates:', err);
      options.onMergeError?.(err);
    },
  });

  return {
    createTemplate,
    updateTemplate,
    deleteTemplate,
    mergeTemplates,
  };
}
