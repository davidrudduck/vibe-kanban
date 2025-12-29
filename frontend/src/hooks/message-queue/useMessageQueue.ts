import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { messageQueueApi } from '@/lib/api';
import type { QueuedMessage } from 'shared/types';

const QUERY_KEY = 'messageQueue';

export function useMessageQueue(attemptId: string | undefined) {
  const queryClient = useQueryClient();
  const queryKey = [QUERY_KEY, attemptId];

  const {
    data: queue = [],
    isLoading,
    refetch,
  } = useQuery({
    queryKey,
    queryFn: () => (attemptId ? messageQueueApi.list(attemptId) : []),
    enabled: !!attemptId,
    staleTime: 5000,
  });

  const addMutation = useMutation({
    mutationFn: ({
      content,
      variant,
    }: {
      content: string;
      variant: string | null;
    }) => {
      if (!attemptId) throw new Error('No attempt ID');
      return messageQueueApi.add(attemptId, content, variant);
    },
    onSuccess: (newMessage) => {
      queryClient.setQueryData<QueuedMessage[]>(queryKey, (old = []) => [
        ...old,
        newMessage,
      ]);
    },
  });

  const updateMutation = useMutation({
    mutationFn: ({
      messageId,
      content,
      variant,
    }: {
      messageId: string;
      content: string | null;
      variant?: string | null;
    }) => {
      if (!attemptId) throw new Error('No attempt ID');
      return messageQueueApi.update(attemptId, messageId, content, variant);
    },
    onSuccess: (updatedMessage) => {
      queryClient.setQueryData<QueuedMessage[]>(queryKey, (old = []) =>
        old.map((msg) => (msg.id === updatedMessage.id ? updatedMessage : msg))
      );
    },
  });

  const removeMutation = useMutation({
    mutationFn: (messageId: string) => {
      if (!attemptId) throw new Error('No attempt ID');
      return messageQueueApi.remove(attemptId, messageId);
    },
    onMutate: async (messageId) => {
      await queryClient.cancelQueries({ queryKey });
      const previousQueue = queryClient.getQueryData<QueuedMessage[]>(queryKey);
      queryClient.setQueryData<QueuedMessage[]>(queryKey, (old = []) =>
        old.filter((msg) => msg.id !== messageId)
      );
      return { previousQueue };
    },
    onError: (_error, _messageId, context) => {
      if (context?.previousQueue) {
        queryClient.setQueryData(queryKey, context.previousQueue);
      }
    },
  });

  const reorderMutation = useMutation({
    mutationFn: (messageIds: string[]) => {
      if (!attemptId) throw new Error('No attempt ID');
      return messageQueueApi.reorder(attemptId, messageIds);
    },
    onSuccess: (reorderedMessages) => {
      queryClient.setQueryData<QueuedMessage[]>(queryKey, reorderedMessages);
    },
  });

  const clearMutation = useMutation({
    mutationFn: () => {
      if (!attemptId) throw new Error('No attempt ID');
      return messageQueueApi.clear(attemptId);
    },
    onSuccess: () => {
      queryClient.setQueryData<QueuedMessage[]>(queryKey, []);
    },
  });

  return {
    queue,
    isLoading,
    queueCount: queue.length,
    addMessage: (content: string, variant: string | null = null) =>
      addMutation.mutateAsync({ content, variant }),
    updateMessage: (
      messageId: string,
      content: string | null,
      variant?: string | null
    ) => updateMutation.mutateAsync({ messageId, content, variant }),
    removeMessage: (messageId: string) => removeMutation.mutateAsync(messageId),
    reorderMessages: (messageIds: string[]) =>
      reorderMutation.mutateAsync(messageIds),
    clearQueue: () => clearMutation.mutateAsync(),
    isAdding: addMutation.isPending,
    isUpdating: updateMutation.isPending,
    isRemoving: removeMutation.isPending,
    isReordering: reorderMutation.isPending,
    isClearing: clearMutation.isPending,
    refetch,
  };
}
