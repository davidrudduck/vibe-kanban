import { describe, it, expect, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import UserMessage from '../UserMessage';
import { TaskAttempt } from 'shared/types';

// Mock dependencies
vi.mock('@/hooks/useProcessRetry', () => ({
  useProcessRetry: () => null,
}));

vi.mock('@/components/ConfigProvider', () => ({
  useUserSystem: () => ({ capabilities: {} }),
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

describe('UserMessage', () => {
  describe('content rendering', () => {
    it('renders message content', () => {
      render(<UserMessage content="Hello world" />);
      expect(screen.getByText('Hello world')).toBeInTheDocument();
    });

    it('renders executor name when taskAttempt is provided', () => {
      const mockTaskAttempt: Partial<TaskAttempt> = {
        id: 'test-id',
        executor: 'CLAUDE_CODE',
      };
      render(
        <UserMessage
          content="Test message"
          taskAttempt={mockTaskAttempt as TaskAttempt}
        />
      );
      expect(screen.getByText('CLAUDE_CODE')).toBeInTheDocument();
    });
  });
});
