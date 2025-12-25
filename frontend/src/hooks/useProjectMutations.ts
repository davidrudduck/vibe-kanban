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

  return {
    createProject,
    updateProject,
    linkLocalFolder,
  };
}
