import { describe, it, expect, beforeEach, vi } from 'vitest';
import { renderHook, waitFor, act } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { createElement } from 'react';
import { useMessageQueue } from './useMessageQueue';
import { messageQueueApi } from '@/lib/api';

// Mock the API
vi.mock('@/lib/api', () => ({
  messageQueueApi: {
    list: vi.fn(),
    add: vi.fn(),
    update: vi.fn(),
    remove: vi.fn(),
    reorder: vi.fn(),
    clear: vi.fn(),
  },
}));

const mockApi = vi.mocked(messageQueueApi);

const createWrapper = () => {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: {
        retry: false,
      },
    },
  });
  return ({ children }: { children: React.ReactNode }) =>
    createElement(QueryClientProvider, { client: queryClient }, children);
};

describe('useMessageQueue', () => {
  const testAttemptId = 'test-attempt-123';

  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('fetching queue', () => {
    it('fetches queue on mount when attemptId provided', async () => {
      const mockMessages = [
        {
          id: '1',
          task_attempt_id: testAttemptId,
          content: 'Message 1',
          variant: null,
          position: 0,
          created_at: '2024-01-01',
        },
        {
          id: '2',
          task_attempt_id: testAttemptId,
          content: 'Message 2',
          variant: null,
          position: 1,
          created_at: '2024-01-01',
        },
      ];
      mockApi.list.mockResolvedValue(mockMessages);

      const { result } = renderHook(() => useMessageQueue(testAttemptId), {
        wrapper: createWrapper(),
      });

      await waitFor(() => {
        expect(result.current.isLoading).toBe(false);
      });

      expect(mockApi.list).toHaveBeenCalledWith(testAttemptId);
      expect(result.current.queue).toEqual(mockMessages);
    });

    it('does not fetch when attemptId is undefined', () => {
      renderHook(() => useMessageQueue(undefined), {
        wrapper: createWrapper(),
      });

      expect(mockApi.list).not.toHaveBeenCalled();
    });
  });

  describe('addMessage', () => {
    it('calls POST and updates cache', async () => {
      const existingMessages = [
        {
          id: '1',
          task_attempt_id: testAttemptId,
          content: 'Message 1',
          variant: null,
          position: 0,
          created_at: '2024-01-01',
        },
      ];
      const newMessage = {
        id: '2',
        task_attempt_id: testAttemptId,
        content: 'New message',
        variant: 'plan',
        position: 1,
        created_at: '2024-01-01',
      };

      mockApi.list.mockResolvedValue(existingMessages);
      mockApi.add.mockResolvedValue(newMessage);

      const { result } = renderHook(() => useMessageQueue(testAttemptId), {
        wrapper: createWrapper(),
      });

      await waitFor(() => {
        expect(result.current.isLoading).toBe(false);
      });

      await act(async () => {
        await result.current.addMessage('New message', 'plan');
      });

      expect(mockApi.add).toHaveBeenCalledWith(
        testAttemptId,
        'New message',
        'plan'
      );
    });
  });

  describe('removeMessage', () => {
    it('performs optimistic update', async () => {
      const initialMessages = [
        {
          id: '1',
          task_attempt_id: testAttemptId,
          content: 'Message 1',
          variant: null,
          position: 0,
          created_at: '2024-01-01',
        },
        {
          id: '2',
          task_attempt_id: testAttemptId,
          content: 'Message 2',
          variant: null,
          position: 1,
          created_at: '2024-01-01',
        },
      ];

      mockApi.list.mockResolvedValue(initialMessages);
      mockApi.remove.mockResolvedValue(undefined);

      const { result } = renderHook(() => useMessageQueue(testAttemptId), {
        wrapper: createWrapper(),
      });

      await waitFor(() => {
        expect(result.current.queue).toHaveLength(2);
      });

      await act(async () => {
        await result.current.removeMessage('1');
      });

      expect(mockApi.remove).toHaveBeenCalledWith(testAttemptId, '1');
    });
  });

  describe('updateMessage', () => {
    it('calls update API with content', async () => {
      const initialMessages = [
        {
          id: '1',
          task_attempt_id: testAttemptId,
          content: 'Message 1',
          variant: null,
          position: 0,
          created_at: '2024-01-01',
        },
      ];
      const updatedMessage = {
        ...initialMessages[0],
        content: 'Updated message',
      };

      mockApi.list.mockResolvedValue(initialMessages);
      mockApi.update.mockResolvedValue(updatedMessage);

      const { result } = renderHook(() => useMessageQueue(testAttemptId), {
        wrapper: createWrapper(),
      });

      await waitFor(() => {
        expect(result.current.isLoading).toBe(false);
      });

      await act(async () => {
        await result.current.updateMessage('1', 'Updated message');
      });

      expect(mockApi.update).toHaveBeenCalledWith(
        testAttemptId,
        '1',
        'Updated message',
        undefined
      );
    });
  });

  describe('reorderMessages', () => {
    it('calls reorder API with new order', async () => {
      const initialMessages = [
        {
          id: '1',
          task_attempt_id: testAttemptId,
          content: 'Message 1',
          variant: null,
          position: 0,
          created_at: '2024-01-01',
        },
        {
          id: '2',
          task_attempt_id: testAttemptId,
          content: 'Message 2',
          variant: null,
          position: 1,
          created_at: '2024-01-01',
        },
      ];
      const reorderedMessages = [initialMessages[1], initialMessages[0]];

      mockApi.list.mockResolvedValue(initialMessages);
      mockApi.reorder.mockResolvedValue(reorderedMessages);

      const { result } = renderHook(() => useMessageQueue(testAttemptId), {
        wrapper: createWrapper(),
      });

      await waitFor(() => {
        expect(result.current.isLoading).toBe(false);
      });

      await act(async () => {
        await result.current.reorderMessages(['2', '1']);
      });

      expect(mockApi.reorder).toHaveBeenCalledWith(testAttemptId, ['2', '1']);
    });
  });

  describe('clearQueue', () => {
    it('calls clear API', async () => {
      const initialMessages = [
        {
          id: '1',
          task_attempt_id: testAttemptId,
          content: 'Message 1',
          variant: null,
          position: 0,
          created_at: '2024-01-01',
        },
      ];

      mockApi.list.mockResolvedValue(initialMessages);
      mockApi.clear.mockResolvedValue(undefined);

      const { result } = renderHook(() => useMessageQueue(testAttemptId), {
        wrapper: createWrapper(),
      });

      await waitFor(() => {
        expect(result.current.isLoading).toBe(false);
      });

      await act(async () => {
        await result.current.clearQueue();
      });

      expect(mockApi.clear).toHaveBeenCalledWith(testAttemptId);
    });
  });
});
