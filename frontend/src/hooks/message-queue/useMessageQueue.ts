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
    onMutate: async ({ content, variant }) => {
      await queryClient.cancelQueries({ queryKey });
      const previousQueue = queryClient.getQueryData<QueuedMessage[]>(queryKey);
      const tempMessage: QueuedMessage = {
        id: `temp-${Date.now()}`,
        task_attempt_id: attemptId!,
        content,
        variant,
        position: previousQueue?.length ?? 0,
        created_at: new Date().toISOString(),
      };
      queryClient.setQueryData<QueuedMessage[]>(queryKey, (old = []) => [
        ...old,
        tempMessage,
      ]);
      return { previousQueue };
    },
    onError: (_error, _vars, context) => {
      if (context?.previousQueue) {
        queryClient.setQueryData(queryKey, context.previousQueue);
      }
    },
    onSuccess: (newMessage) => {
      queryClient.setQueryData<QueuedMessage[]>(queryKey, (old = []) =>
        old.map((msg) => (msg.id.startsWith('temp-') ? newMessage : msg))
      );
    },
    onSettled: () => {
      queryClient.invalidateQueries({ queryKey });
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
    onMutate: async ({ messageId, content, variant }) => {
      await queryClient.cancelQueries({ queryKey });
      const previousQueue = queryClient.getQueryData<QueuedMessage[]>(queryKey);
      queryClient.setQueryData<QueuedMessage[]>(queryKey, (old = []) =>
        old.map((msg) =>
          msg.id === messageId
            ? {
                ...msg,
                content: content ?? msg.content,
                variant: variant !== undefined ? variant : msg.variant,
              }
            : msg
        )
      );
      return { previousQueue };
    },
    onError: (_error, _vars, context) => {
      if (context?.previousQueue) {
        queryClient.setQueryData(queryKey, context.previousQueue);
      }
    },
    onSuccess: (updatedMessage) => {
      queryClient.setQueryData<QueuedMessage[]>(queryKey, (old = []) =>
        old.map((msg) => (msg.id === updatedMessage.id ? updatedMessage : msg))
      );
    },
    onSettled: () => {
      queryClient.invalidateQueries({ queryKey });
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
    onSettled: () => {
      queryClient.invalidateQueries({ queryKey });
    },
  });

  const reorderMutation = useMutation({
    mutationFn: (messageIds: string[]) => {
      if (!attemptId) throw new Error('No attempt ID');
      return messageQueueApi.reorder(attemptId, messageIds);
    },
    onMutate: async (messageIds) => {
      await queryClient.cancelQueries({ queryKey });
      const previousQueue = queryClient.getQueryData<QueuedMessage[]>(queryKey);
      // Reorder optimistically based on the new order
      if (previousQueue) {
        const messageMap = new Map(previousQueue.map((msg) => [msg.id, msg]));
        const reorderedQueue = messageIds
          .map((id, index) => {
            const msg = messageMap.get(id);
            return msg ? { ...msg, position: index } : null;
          })
          .filter((msg): msg is QueuedMessage => msg !== null);
        queryClient.setQueryData<QueuedMessage[]>(queryKey, reorderedQueue);
      }
      return { previousQueue };
    },
    onError: (_error, _messageIds, context) => {
      if (context?.previousQueue) {
        queryClient.setQueryData(queryKey, context.previousQueue);
      }
    },
    onSuccess: (reorderedMessages) => {
      queryClient.setQueryData<QueuedMessage[]>(queryKey, reorderedMessages);
    },
    onSettled: () => {
      queryClient.invalidateQueries({ queryKey });
    },
  });

  const clearMutation = useMutation({
    mutationFn: () => {
      if (!attemptId) throw new Error('No attempt ID');
      return messageQueueApi.clear(attemptId);
    },
    onMutate: async () => {
      await queryClient.cancelQueries({ queryKey });
      const previousQueue = queryClient.getQueryData<QueuedMessage[]>(queryKey);
      queryClient.setQueryData<QueuedMessage[]>(queryKey, []);
      return { previousQueue };
    },
    onError: (_error, _vars, context) => {
      if (context?.previousQueue) {
        queryClient.setQueryData(queryKey, context.previousQueue);
      }
    },
    onSettled: () => {
      queryClient.invalidateQueries({ queryKey });
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
