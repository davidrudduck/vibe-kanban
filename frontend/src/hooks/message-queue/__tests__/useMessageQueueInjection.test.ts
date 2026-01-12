import { describe, it, expect, beforeEach, vi } from 'vitest';
import { renderHook, waitFor, act } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { createElement } from 'react';
import { useMessageQueueInjection } from '../useMessageQueueInjection';
import { messageQueueApi, executionProcessesApi } from '@/lib/api';

// Mock the APIs
vi.mock('@/lib/api', () => ({
  messageQueueApi: {
    list: vi.fn(),
    add: vi.fn(),
    update: vi.fn(),
    remove: vi.fn(),
    reorder: vi.fn(),
    clear: vi.fn(),
  },
  executionProcessesApi: {
    injectMessage: vi.fn(),
  },
}));

const mockMessageQueueApi = vi.mocked(messageQueueApi);
const mockExecutionProcessesApi = vi.mocked(executionProcessesApi);

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

describe('useMessageQueueInjection', () => {
  const testAttemptId = 'test-attempt-123';
  const testProcessId = 'test-process-456';

  beforeEach(() => {
    vi.clearAllMocks();
    // Default mock for list to prevent query errors
    mockMessageQueueApi.list.mockResolvedValue([]);
  });

  describe('addAndInject', () => {
    it('removes message from queue after successful injection', async () => {
      const mockMessage = {
        id: 'msg-1',
        task_attempt_id: testAttemptId,
        content: 'test message',
        variant: null,
        position: 0,
        created_at: '2024-01-01',
      };
      mockMessageQueueApi.add.mockResolvedValue(mockMessage);
      mockExecutionProcessesApi.injectMessage.mockResolvedValue({
        injected: true,
      });
      mockMessageQueueApi.remove.mockResolvedValue(undefined);

      const { result } = renderHook(
        () => useMessageQueueInjection(testAttemptId, testProcessId),
        { wrapper: createWrapper() }
      );

      await waitFor(() => {
        expect(result.current.isLoading).toBe(false);
      });

      let response: { queued: boolean; injected: boolean };
      await act(async () => {
        response = await result.current.addAndInject('test message', null);
      });

      expect(response!).toEqual({ queued: false, injected: true });
      expect(mockMessageQueueApi.add).toHaveBeenCalledWith(
        testAttemptId,
        'test message',
        null
      );
      expect(mockExecutionProcessesApi.injectMessage).toHaveBeenCalledWith(
        testProcessId,
        'test message'
      );
      expect(mockMessageQueueApi.remove).toHaveBeenCalledWith(
        testAttemptId,
        'msg-1'
      );
    });

    it('keeps message in queue when injection returns false', async () => {
      const mockMessage = {
        id: 'msg-1',
        task_attempt_id: testAttemptId,
        content: 'test message',
        variant: null,
        position: 0,
        created_at: '2024-01-01',
      };
      mockMessageQueueApi.add.mockResolvedValue(mockMessage);
      mockExecutionProcessesApi.injectMessage.mockResolvedValue({
        injected: false,
      });

      const { result } = renderHook(
        () => useMessageQueueInjection(testAttemptId, testProcessId),
        { wrapper: createWrapper() }
      );

      await waitFor(() => {
        expect(result.current.isLoading).toBe(false);
      });

      let response: { queued: boolean; injected: boolean };
      await act(async () => {
        response = await result.current.addAndInject('test message', null);
      });

      expect(response!).toEqual({ queued: true, injected: false });
      expect(mockMessageQueueApi.remove).not.toHaveBeenCalled();
    });

    it('keeps message in queue when injection throws error', async () => {
      const mockMessage = {
        id: 'msg-1',
        task_attempt_id: testAttemptId,
        content: 'test message',
        variant: null,
        position: 0,
        created_at: '2024-01-01',
      };
      mockMessageQueueApi.add.mockResolvedValue(mockMessage);
      mockExecutionProcessesApi.injectMessage.mockRejectedValue(
        new Error('Network error')
      );

      // Suppress console.error for this test
      const consoleSpy = vi.spyOn(console, 'error').mockImplementation(() => {});

      const { result } = renderHook(
        () => useMessageQueueInjection(testAttemptId, testProcessId),
        { wrapper: createWrapper() }
      );

      await waitFor(() => {
        expect(result.current.isLoading).toBe(false);
      });

      let response: { queued: boolean; injected: boolean };
      await act(async () => {
        response = await result.current.addAndInject('test message', null);
      });

      expect(response!).toEqual({ queued: true, injected: false });
      expect(mockMessageQueueApi.remove).not.toHaveBeenCalled();
      expect(result.current.lastInjectionError).toBeInstanceOf(Error);
      expect(result.current.lastInjectionError?.message).toBe('Network error');

      consoleSpy.mockRestore();
    });

    it('does not attempt injection when no running process', async () => {
      const mockMessage = {
        id: 'msg-1',
        task_attempt_id: testAttemptId,
        content: 'test message',
        variant: null,
        position: 0,
        created_at: '2024-01-01',
      };
      mockMessageQueueApi.add.mockResolvedValue(mockMessage);

      const { result } = renderHook(
        () => useMessageQueueInjection(testAttemptId, undefined),
        { wrapper: createWrapper() }
      );

      await waitFor(() => {
        expect(result.current.isLoading).toBe(false);
      });

      let response: { queued: boolean; injected: boolean };
      await act(async () => {
        response = await result.current.addAndInject('test message', null);
      });

      expect(response!).toEqual({ queued: true, injected: false });
      expect(mockExecutionProcessesApi.injectMessage).not.toHaveBeenCalled();
    });

    it('passes variant to addMessage', async () => {
      const mockMessage = {
        id: 'msg-1',
        task_attempt_id: testAttemptId,
        content: 'test message',
        variant: 'plan',
        position: 0,
        created_at: '2024-01-01',
      };
      mockMessageQueueApi.add.mockResolvedValue(mockMessage);
      mockExecutionProcessesApi.injectMessage.mockResolvedValue({
        injected: true,
      });
      mockMessageQueueApi.remove.mockResolvedValue(undefined);

      const { result } = renderHook(
        () => useMessageQueueInjection(testAttemptId, testProcessId),
        { wrapper: createWrapper() }
      );

      await waitFor(() => {
        expect(result.current.isLoading).toBe(false);
      });

      await act(async () => {
        await result.current.addAndInject('test message', 'plan');
      });

      expect(mockMessageQueueApi.add).toHaveBeenCalledWith(
        testAttemptId,
        'test message',
        'plan'
      );
    });
  });

  describe('injectOnly', () => {
    it('injects message without adding to queue', async () => {
      mockExecutionProcessesApi.injectMessage.mockResolvedValue({
        injected: true,
      });

      const { result } = renderHook(
        () => useMessageQueueInjection(testAttemptId, testProcessId),
        { wrapper: createWrapper() }
      );

      await waitFor(() => {
        expect(result.current.isLoading).toBe(false);
      });

      let response: { injected: boolean };
      await act(async () => {
        response = await result.current.injectOnly('direct message');
      });

      expect(response!).toEqual({ injected: true });
      expect(mockExecutionProcessesApi.injectMessage).toHaveBeenCalledWith(
        testProcessId,
        'direct message'
      );
      expect(mockMessageQueueApi.add).not.toHaveBeenCalled();
    });

    it('returns false when no running process', async () => {
      const { result } = renderHook(
        () => useMessageQueueInjection(testAttemptId, undefined),
        { wrapper: createWrapper() }
      );

      await waitFor(() => {
        expect(result.current.isLoading).toBe(false);
      });

      let response: { injected: boolean };
      await act(async () => {
        response = await result.current.injectOnly('direct message');
      });

      expect(response!).toEqual({ injected: false });
      expect(mockExecutionProcessesApi.injectMessage).not.toHaveBeenCalled();
    });

    it('handles injection error gracefully', async () => {
      mockExecutionProcessesApi.injectMessage.mockRejectedValue(
        new Error('Injection failed')
      );

      // Suppress console.error for this test
      const consoleSpy = vi.spyOn(console, 'error').mockImplementation(() => {});

      const { result } = renderHook(
        () => useMessageQueueInjection(testAttemptId, testProcessId),
        { wrapper: createWrapper() }
      );

      await waitFor(() => {
        expect(result.current.isLoading).toBe(false);
      });

      let response: { injected: boolean };
      await act(async () => {
        response = await result.current.injectOnly('direct message');
      });

      expect(response!).toEqual({ injected: false });
      expect(result.current.lastInjectionError).toBeInstanceOf(Error);
      expect(result.current.lastInjectionError?.message).toBe('Injection failed');

      consoleSpy.mockRestore();
    });
  });

  describe('canInject', () => {
    it('returns true when running process exists', async () => {
      const { result } = renderHook(
        () => useMessageQueueInjection(testAttemptId, testProcessId),
        { wrapper: createWrapper() }
      );

      expect(result.current.canInject).toBe(true);
    });

    it('returns false when no running process', async () => {
      const { result } = renderHook(
        () => useMessageQueueInjection(testAttemptId, undefined),
        { wrapper: createWrapper() }
      );

      expect(result.current.canInject).toBe(false);
    });
  });

  describe('isInjecting state', () => {
    it('sets isInjecting to true during injection', async () => {
      let resolveInjection: ((value: { injected: boolean }) => void) | undefined;
      mockExecutionProcessesApi.injectMessage.mockImplementation(
        () =>
          new Promise((resolve) => {
            resolveInjection = resolve;
          })
      );
      const mockMessage = {
        id: 'msg-1',
        task_attempt_id: testAttemptId,
        content: 'test message',
        variant: null,
        position: 0,
        created_at: '2024-01-01',
      };
      mockMessageQueueApi.add.mockResolvedValue(mockMessage);
      mockMessageQueueApi.remove.mockResolvedValue(undefined);

      const { result } = renderHook(
        () => useMessageQueueInjection(testAttemptId, testProcessId),
        { wrapper: createWrapper() }
      );

      await waitFor(() => {
        expect(result.current.isLoading).toBe(false);
      });

      expect(result.current.isInjecting).toBe(false);

      // Start the injection but don't await it
      let injectionPromise: Promise<{ queued: boolean; injected: boolean }>;
      act(() => {
        injectionPromise = result.current.addAndInject('test message', null);
      });

      // Wait for isInjecting to become true
      await waitFor(() => {
        expect(result.current.isInjecting).toBe(true);
      });

      // Resolve the injection
      await act(async () => {
        resolveInjection!({ injected: true });
        await injectionPromise;
      });

      expect(result.current.isInjecting).toBe(false);
    });
  });

  describe('inherited messageQueue functionality', () => {
    it('exposes queue from useMessageQueue', async () => {
      const mockMessages = [
        {
          id: '1',
          task_attempt_id: testAttemptId,
          content: 'Message 1',
          variant: null,
          position: 0,
          created_at: '2024-01-01',
        },
      ];
      mockMessageQueueApi.list.mockResolvedValue(mockMessages);

      const { result } = renderHook(
        () => useMessageQueueInjection(testAttemptId, testProcessId),
        { wrapper: createWrapper() }
      );

      await waitFor(() => {
        expect(result.current.isLoading).toBe(false);
      });

      expect(result.current.queue).toEqual(mockMessages);
      expect(result.current.queueCount).toBe(1);
    });
  });
});
