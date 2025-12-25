import { useMutation, useQueryClient } from '@tanstack/react-query';
import { projectsApi } from '@/lib/api';
import type {
  CreateProject,
  UpdateProject,
  Project,
  LinkToLocalFolderRequest,
} from 'shared/types';

interface UseProjectMutationsOptions {
  onCreateSuccess?: (project: Project) => void;
  onCreateError?: (err: unknown) => void;
  onUpdateSuccess?: (project: Project) => void;
  onUpdateError?: (err: unknown) => void;
  onLinkLocalFolderSuccess?: (project: Project) => void;
  onLinkLocalFolderError?: (err: unknown) => void;
  // Legacy link/unlink options (deprecated - API not implemented)
  onLinkSuccess?: () => void;
  onLinkError?: (err: unknown) => void;
  onUnlinkSuccess?: () => void;
  onUnlinkError?: (err: unknown) => void;
}

export function useProjectMutations(options?: UseProjectMutationsOptions) {
  const queryClient = useQueryClient();

  const createProject = useMutation({
    mutationKey: ['createProject'],
    mutationFn: (data: CreateProject) => projectsApi.create(data),
    onSuccess: (project: Project) => {
      queryClient.setQueryData(['project', project.id], project);
      queryClient.invalidateQueries({ queryKey: ['projects'] });
      options?.onCreateSuccess?.(project);
    },
    onError: (err) => {
      console.error('Failed to create project:', err);
      options?.onCreateError?.(err);
    },
  });

  const updateProject = useMutation({
    mutationKey: ['updateProject'],
    mutationFn: ({
      projectId,
      data,
    }: {
      projectId: string;
      data: UpdateProject;
    }) => projectsApi.update(projectId, data),
    onSuccess: (project: Project) => {
      // Update single project cache
      queryClient.setQueryData(['project', project.id], project);

      // Update the project in the projects list cache immediately
      queryClient.setQueryData<Project[]>(['projects'], (old) => {
        if (!old) return old;
        return old.map((p) => (p.id === project.id ? project : p));
      });

      options?.onUpdateSuccess?.(project);
    },
    onError: (err) => {
      console.error('Failed to update project:', err);
      options?.onUpdateError?.(err);
    },
  });

  const linkLocalFolder = useMutation({
    mutationKey: ['linkLocalFolder'],
    mutationFn: (data: LinkToLocalFolderRequest) =>
      projectsApi.linkLocalFolder(data),
    onSuccess: (project: Project) => {
      queryClient.setQueryData(['project', project.id], project);

      // Invalidate to ensure fresh data from server
      queryClient.invalidateQueries({ queryKey: ['projects'] });
      queryClient.invalidateQueries({ queryKey: ['mergedProjects'] });
      queryClient.invalidateQueries({ queryKey: ['unifiedProjects'] });

      // Invalidate organization projects queries since linking affects remote projects
      queryClient.invalidateQueries({
        queryKey: ['organizations'],
        predicate: (query) => {
          const key = query.queryKey;
          return (
            key.length === 3 &&
            key[0] === 'organizations' &&
            key[2] === 'projects'
          );
        },
      });

      options?.onLinkLocalFolderSuccess?.(project);
    },
    onError: (err) => {
      console.error('Failed to link local folder:', err);
      options?.onLinkLocalFolderError?.(err);
    },
  });

  // Stub mutations for link/unlink (API not yet implemented)
  // These are referenced by OrganizationSettings but don't work
  const linkToExisting = useMutation({
    mutationKey: ['linkToExisting'],
    mutationFn: async (data: {
      localProjectId: string;
      data: { remote_project_id: string };
    }) => {
      console.warn(
        'Link to existing API not implemented',
        data.localProjectId
      );
      throw new Error('Link to existing API not implemented');
    },
    onError: (err) => {
      console.error('Link to existing not implemented:', err);
      options?.onLinkError?.(err);
    },
  });

  const unlinkProject = useMutation({
    mutationKey: ['unlinkProject'],
    mutationFn: async (projectId: string) => {
      console.warn('Unlink project API not implemented', projectId);
      throw new Error('Unlink project API not implemented');
    },
    onError: (err) => {
      console.error('Unlink project not implemented:', err);
      options?.onUnlinkError?.(err);
    },
  });

  return {
    createProject,
    updateProject,
    linkLocalFolder,
    linkToExisting,
    unlinkProject,
  };
}
