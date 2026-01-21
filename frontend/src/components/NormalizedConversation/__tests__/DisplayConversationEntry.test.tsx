import { describe, it, expect, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import DisplayConversationEntry from '../DisplayConversationEntry';
import { ExecutionProcess } from 'shared/types';

// Create mutable mock data
let mockExecutionProcessesById: Record<string, ExecutionProcess> = {
  'test-process-id': {
    id: 'test-process-id',
    executor_action: {
      typ: {
        type: 'CodingAgentInitialRequest',
        executor_profile_id: {
          executor: 'CLAUDE_CODE',
          variant: 'PLAN',
        },
      },
    },
  } as ExecutionProcess,
};

vi.mock('@/contexts/ExecutionProcessesContext', () => ({
  useExecutionProcessesContext: () => ({
    get executionProcessesByIdAll() {
      return mockExecutionProcessesById;
    },
    get executionProcessesAll() {
      return Object.values(mockExecutionProcessesById);
    },
    get executionProcessesByIdVisible() {
      return mockExecutionProcessesById;
    },
    get executionProcessesVisible() {
      return Object.values(mockExecutionProcessesById);
    },
    isAttemptRunningAll: false,
    isAttemptRunningVisible: false,
    isLoading: false,
    isConnected: true,
    error: null,
  }),
}));

// Mock other required dependencies
vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string) => key,
  }),
}));

vi.mock('@/components/ConfigProvider', () => ({
  useUserSystem: () => ({
    capabilities: {
      CLAUDE_CODE: ['session.fork']
    }
  }),
}));

vi.mock('@/hooks/useProcessRetry', () => ({
  useProcessRetry: () => null,
}));

vi.mock('@/hooks/follow-up/useDraftStream', () => ({
  useDraftStream: () => ({ retryDraft: null }),
}));

vi.mock('@/contexts/RetryUiContext', () => ({
  useRetryUi: () => ({
    activeRetryProcessId: null,
    isProcessGreyed: () => false,
  }),
}));

vi.mock('@/contexts/FileViewerContext', () => ({
  useFileViewer: () => ({
    openFile: vi.fn(),
  }),
}));

describe('DisplayConversationEntry - Executor Variant Integration', () => {
  it('displays executor variant from ExecutionProcess data', () => {
    const entry = {
      entry_index: 0,
      content: 'Test message',
      entry_type: {
        type: 'user_message',
      },
      timestamp: new Date().toISOString(),
    } as any;

    const mockTaskAttempt = {
      id: 'attempt-123',
      executor: 'CLAUDE_CODE',
    };

    render(
      <DisplayConversationEntry
        entry={entry}
        expansionKey="test-key"
        executionProcessId="test-process-id"
        taskAttempt={mockTaskAttempt as any}
      />
    );

    // Verify that the variant "PLAN" is displayed alongside the executor
    expect(screen.getByText('CLAUDE_CODE / PLAN')).toBeInTheDocument();
  });

  it('displays only executor when ExecutionProcess has no variant', () => {
    // Update the mock data to have no variant
    mockExecutionProcessesById = {
      'no-variant-id': {
        id: 'no-variant-id',
        executor_action: {
          typ: {
            type: 'CodingAgentInitialRequest',
            executor_profile_id: {
              executor: 'CLAUDE_CODE',
              // No variant
            },
          },
        },
      } as ExecutionProcess,
    };

    const entry = {
      entry_index: 0,
      content: 'Test message',
      entry_type: {
        type: 'user_message',
      },
      timestamp: new Date().toISOString(),
    } as any;

    const mockTaskAttempt = {
      id: 'attempt-123',
      executor: 'CLAUDE_CODE',
    };

    render(
      <DisplayConversationEntry
        entry={entry}
        expansionKey="test-key-2"
        executionProcessId="no-variant-id"
        taskAttempt={mockTaskAttempt as any}
      />
    );

    // Should show executor only, without variant
    expect(screen.getByText('CLAUDE_CODE')).toBeInTheDocument();
    // Verify there's no slash (which would indicate "EXECUTOR / VARIANT")
    const text = screen.getByText('CLAUDE_CODE').textContent || '';
    expect(text).not.toContain('/');
  });
});
