import { useMutation, useQueryClient } from '@tanstack/react-query';
import { dashboardApi } from '@/lib/api';

interface UseActivityDismissOptions {
  onDismissSuccess?: () => void;
  onDismissError?: (error: unknown) => void;
  onUndismissSuccess?: () => void;
  onUndismissError?: (error: unknown) => void;
}

export function useActivityDismiss(options?: UseActivityDismissOptions) {
  const queryClient = useQueryClient();

  const dismissMutation = useMutation({
    mutationKey: ['activity', 'dismiss'],
    mutationFn: (taskId: string) => dashboardApi.dismissActivityItem(taskId),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['dashboard', 'activity'] });
      options?.onDismissSuccess?.();
    },
    onError: (error) => {
      console.error('Failed to dismiss activity item:', error);
      options?.onDismissError?.(error);
    },
  });

  const undismissMutation = useMutation({
    mutationKey: ['activity', 'undismiss'],
    mutationFn: (taskId: string) => dashboardApi.undismissActivityItem(taskId),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['dashboard', 'activity'] });
      options?.onUndismissSuccess?.();
    },
    onError: (error) => {
      console.error('Failed to undismiss activity item:', error);
      options?.onUndismissError?.(error);
    },
  });

  return {
    dismissItem: dismissMutation,
    undismissItem: undismissMutation,
  };
}
