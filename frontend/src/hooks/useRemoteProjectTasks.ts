import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { remoteProjectsApi } from '@/lib/api';
import type {
  BulkSharedTasksResponse,
  SharedTaskResponse,
  HiveSharedTaskWithUser,
} from '@/types/remote';
import type {
  RemoteProjectInfo,
  CreateRemoteTaskRequest,
  UpdateRemoteTaskRequest,
  AssignRemoteTaskRequest,
} from 'shared/types';

/**
 * Hook to fetch and manage tasks for a remote project.
 * All operations are proxied through the Hive.
 */
export function useRemoteProjectTasks(projectId: string | undefined) {
  const queryClient = useQueryClient();

  // Fetch project info
  const projectInfoQuery = useQuery<RemoteProjectInfo>({
    queryKey: ['remote-project-info', projectId],
    queryFn: () => remoteProjectsApi.getInfo(projectId!),
    enabled: !!projectId,
    staleTime: 60000, // Cache for 1 minute
  });

  // Fetch tasks
  const tasksQuery = useQuery<BulkSharedTasksResponse>({
    queryKey: ['remote-project-tasks', projectId],
    queryFn: () => remoteProjectsApi.getTasks(projectId!),
    enabled: !!projectId,
    staleTime: 10000, // Refetch after 10 seconds
  });

  // Create task mutation
  const createTask = useMutation<SharedTaskResponse, Error, CreateRemoteTaskRequest>({
    mutationFn: (data) => remoteProjectsApi.createTask(projectId!, data),
    onSuccess: () => {
      queryClient.invalidateQueries({
        queryKey: ['remote-project-tasks', projectId],
      });
    },
  });

  // Update task mutation
  const updateTask = useMutation<
    SharedTaskResponse,
    Error,
    { taskId: string; data: UpdateRemoteTaskRequest }
  >({
    mutationFn: ({ taskId, data }) =>
      remoteProjectsApi.updateTask(projectId!, taskId, data),
    onSuccess: () => {
      queryClient.invalidateQueries({
        queryKey: ['remote-project-tasks', projectId],
      });
    },
  });

  // Delete task mutation
  const deleteTask = useMutation<SharedTaskResponse, Error, string>({
    mutationFn: (taskId) => remoteProjectsApi.deleteTask(projectId!, taskId),
    onSuccess: () => {
      queryClient.invalidateQueries({
        queryKey: ['remote-project-tasks', projectId],
      });
    },
  });

  // Assign task mutation
  const assignTask = useMutation<
    SharedTaskResponse,
    Error,
    { taskId: string; data: AssignRemoteTaskRequest }
  >({
    mutationFn: ({ taskId, data }) =>
      remoteProjectsApi.assignTask(projectId!, taskId, data),
    onSuccess: () => {
      queryClient.invalidateQueries({
        queryKey: ['remote-project-tasks', projectId],
      });
    },
  });

  // Helper to group tasks by status
  const groupedTasks = tasksQuery.data?.tasks.reduce(
    (acc, taskWithUser) => {
      const status = taskWithUser.task.status;
      if (!acc[status]) {
        acc[status] = [];
      }
      acc[status].push(taskWithUser);
      return acc;
    },
    {} as Record<string, HiveSharedTaskWithUser[]>
  );

  return {
    // Project info
    projectInfo: projectInfoQuery.data,
    projectInfoLoading: projectInfoQuery.isLoading,
    projectInfoError: projectInfoQuery.error,

    // Tasks
    tasks: tasksQuery.data?.tasks ?? [],
    groupedTasks,
    tasksLoading: tasksQuery.isLoading,
    tasksError: tasksQuery.error,
    refetchTasks: tasksQuery.refetch,

    // Mutations
    createTask,
    updateTask,
    deleteTask,
    assignTask,
  };
}
