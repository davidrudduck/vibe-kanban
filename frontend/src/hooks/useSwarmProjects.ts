import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { swarmProjectsApi } from '@/lib/api';
import type {
  SwarmProject,
  SwarmProjectWithNodes,
  SwarmProjectNode,
  CreateSwarmProjectRequest,
  UpdateSwarmProjectRequest,
  LinkSwarmProjectNodeRequest,
} from '@/types/swarm';

// Query keys for cache management
export const swarmProjectsKeys = {
  all: ['swarmProjects'] as const,
  lists: () => [...swarmProjectsKeys.all, 'list'] as const,
  list: (organizationId: string) =>
    [...swarmProjectsKeys.lists(), organizationId] as const,
  details: () => [...swarmProjectsKeys.all, 'detail'] as const,
  detail: (id: string) => [...swarmProjectsKeys.details(), id] as const,
  nodes: (projectId: string) =>
    [...swarmProjectsKeys.all, 'nodes', projectId] as const,
};

interface UseSwarmProjectsOptions {
  organizationId: string;
  enabled?: boolean;
}

/**
 * Hook to fetch and cache swarm projects for an organization.
 */
export function useSwarmProjects({
  organizationId,
  enabled = true,
}: UseSwarmProjectsOptions) {
  return useQuery({
    queryKey: swarmProjectsKeys.list(organizationId),
    queryFn: () => swarmProjectsApi.list(organizationId),
    enabled: enabled && !!organizationId,
    staleTime: 30_000, // Consider data stale after 30 seconds
  });
}

interface UseSwarmProjectOptions {
  projectId: string;
  enabled?: boolean;
}

/**
 * Hook to fetch a single swarm project by ID.
 */
export function useSwarmProject({
  projectId,
  enabled = true,
}: UseSwarmProjectOptions) {
  return useQuery({
    queryKey: swarmProjectsKeys.detail(projectId),
    queryFn: () => swarmProjectsApi.getById(projectId),
    enabled: enabled && !!projectId,
  });
}

/**
 * Hook to fetch nodes linked to a swarm project.
 */
export function useSwarmProjectNodes({
  projectId,
  enabled = true,
}: UseSwarmProjectOptions) {
  return useQuery({
    queryKey: swarmProjectsKeys.nodes(projectId),
    queryFn: () => swarmProjectsApi.listNodes(projectId),
    enabled: enabled && !!projectId,
  });
}

interface UseSwarmProjectMutationsOptions {
  organizationId: string;
  onCreateSuccess?: (project: SwarmProject) => void;
  onCreateError?: (err: unknown) => void;
  onUpdateSuccess?: (project: SwarmProject) => void;
  onUpdateError?: (err: unknown) => void;
  onDeleteSuccess?: () => void;
  onDeleteError?: (err: unknown) => void;
  onMergeSuccess?: (project: SwarmProject) => void;
  onMergeError?: (err: unknown) => void;
  onLinkNodeSuccess?: (link: SwarmProjectNode) => void;
  onLinkNodeError?: (err: unknown) => void;
  onUnlinkNodeSuccess?: () => void;
  onUnlinkNodeError?: (err: unknown) => void;
}

/**
 * Hook providing mutations for swarm project CRUD operations.
 */
export function useSwarmProjectMutations(
  options: UseSwarmProjectMutationsOptions
) {
  const queryClient = useQueryClient();
  const { organizationId } = options;

  const invalidateProjects = () => {
    queryClient.invalidateQueries({
      queryKey: swarmProjectsKeys.list(organizationId),
    });
  };

  const createProject = useMutation({
    mutationKey: ['createSwarmProject'],
    mutationFn: (data: CreateSwarmProjectRequest) =>
      swarmProjectsApi.create(data),
    onSuccess: (project: SwarmProject) => {
      queryClient.setQueryData(swarmProjectsKeys.detail(project.id), project);
      invalidateProjects();
      options.onCreateSuccess?.(project);
    },
    onError: (err) => {
      console.error('Failed to create swarm project:', err);
      options.onCreateError?.(err);
    },
  });

  const updateProject = useMutation({
    mutationKey: ['updateSwarmProject'],
    mutationFn: ({
      projectId,
      data,
    }: {
      projectId: string;
      data: UpdateSwarmProjectRequest;
    }) => swarmProjectsApi.update(projectId, data),
    onSuccess: (project: SwarmProject) => {
      queryClient.setQueryData(swarmProjectsKeys.detail(project.id), project);
      // Update in list cache
      queryClient.setQueryData<SwarmProjectWithNodes[]>(
        swarmProjectsKeys.list(organizationId),
        (old) => {
          if (!old) return old;
          return old.map((p) =>
            p.id === project.id
              ? {
                  ...project,
                  linked_nodes_count: p.linked_nodes_count,
                  linked_node_names: p.linked_node_names,
                }
              : p
          );
        }
      );
      options.onUpdateSuccess?.(project);
    },
    onError: (err) => {
      console.error('Failed to update swarm project:', err);
      options.onUpdateError?.(err);
    },
  });

  const deleteProject = useMutation({
    mutationKey: ['deleteSwarmProject'],
    mutationFn: (projectId: string) => swarmProjectsApi.delete(projectId),
    onSuccess: (_, projectId) => {
      queryClient.removeQueries({
        queryKey: swarmProjectsKeys.detail(projectId),
      });
      invalidateProjects();
      options.onDeleteSuccess?.();
    },
    onError: (err) => {
      console.error('Failed to delete swarm project:', err);
      options.onDeleteError?.(err);
    },
  });

  const mergeProjects = useMutation({
    mutationKey: ['mergeSwarmProjects'],
    mutationFn: ({
      targetId,
      sourceId,
    }: {
      targetId: string;
      sourceId: string;
    }) => swarmProjectsApi.merge(targetId, { source_id: sourceId }),
    onSuccess: (project: SwarmProject, { sourceId }) => {
      // Remove the source project from cache
      queryClient.removeQueries({
        queryKey: swarmProjectsKeys.detail(sourceId),
      });
      queryClient.setQueryData(swarmProjectsKeys.detail(project.id), project);
      invalidateProjects();
      options.onMergeSuccess?.(project);
    },
    onError: (err) => {
      console.error('Failed to merge swarm projects:', err);
      options.onMergeError?.(err);
    },
  });

  const linkNode = useMutation({
    mutationKey: ['linkSwarmProjectNode'],
    mutationFn: ({
      projectId,
      data,
    }: {
      projectId: string;
      data: LinkSwarmProjectNodeRequest;
    }) => swarmProjectsApi.linkNode(projectId, data),
    onSuccess: (link: SwarmProjectNode) => {
      queryClient.invalidateQueries({
        queryKey: swarmProjectsKeys.nodes(link.swarm_project_id),
      });
      invalidateProjects();
      options.onLinkNodeSuccess?.(link);
    },
    onError: (err) => {
      console.error('Failed to link node to swarm project:', err);
      options.onLinkNodeError?.(err);
    },
  });

  const unlinkNode = useMutation({
    mutationKey: ['unlinkSwarmProjectNode'],
    mutationFn: ({
      projectId,
      nodeId,
    }: {
      projectId: string;
      nodeId: string;
    }) => swarmProjectsApi.unlinkNode(projectId, nodeId),
    onSuccess: (_, { projectId }) => {
      queryClient.invalidateQueries({
        queryKey: swarmProjectsKeys.nodes(projectId),
      });
      invalidateProjects();
      options.onUnlinkNodeSuccess?.();
    },
    onError: (err) => {
      console.error('Failed to unlink node from swarm project:', err);
      options.onUnlinkNodeError?.(err);
    },
  });

  return {
    createProject,
    updateProject,
    deleteProject,
    mergeProjects,
    linkNode,
    unlinkNode,
  };
}
