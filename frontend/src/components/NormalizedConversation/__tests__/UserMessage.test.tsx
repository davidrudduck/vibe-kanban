import { describe, it, expect, vi } from 'vitest';
import { render, screen, act } from '@testing-library/react';
import UserMessage from '../UserMessage';
import { TaskAttempt } from 'shared/types';

function createMockTaskAttempt(overrides?: Partial<TaskAttempt>): TaskAttempt {
  return {
    id: 'test-id',
    task_id: 'task-123',
    container_ref: null,
    branch: 'feature-branch',
    target_branch: 'main',
    executor: 'CLAUDE_CODE',
    worktree_deleted: false,
    setup_completed_at: null,
    created_at: new Date().toISOString(),
    updated_at: new Date().toISOString(),
    ...overrides,
  };
}

// Mock i18next
vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string) => key,
  }),
}));

// Mock dependencies
vi.mock('@/hooks/useProcessRetry', () => ({
  useProcessRetry: () => null,
}));

vi.mock('@/components/ConfigProvider', () => ({
  useUserSystem: () => ({
    capabilities: {
      CLAUDE_CODE: ['session.fork'],
    },
  }),
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
      const mockTaskAttempt = createMockTaskAttempt();
      render(
        <UserMessage content="Test message" taskAttempt={mockTaskAttempt} />
      );
      expect(screen.getByText('CLAUDE_CODE')).toBeInTheDocument();
    });
  });

  describe('styling consistency', () => {
    it('renders content with text-sm class for font size consistency', () => {
      const { container } = render(<UserMessage content="Test message" />);
      const contentDiv = container.querySelector('.text-sm');
      expect(contentDiv).toBeInTheDocument();
    });
  });

  describe('edit button visibility', () => {
    it('has group class on outer container for hover effects', () => {
      const mockTaskAttempt = createMockTaskAttempt();
      const { container } = render(
        <UserMessage
          content="Test"
          executionProcessId="exec-1"
          taskAttempt={mockTaskAttempt}
        />
      );
      const outerContainer = container.querySelector('.group.border');
      expect(outerContainer).toBeInTheDocument();
    });
  });

  describe('expand/collapse chevron', () => {
    it('renders chevron at bottom of content when message exceeds 5 lines', () => {
      const longContent = 'Line 1\nLine 2\nLine 3\nLine 4\nLine 5\nLine 6';
      render(<UserMessage content={longContent} />);
      // Look for expand message aria-label using translation key
      const chevronButton = screen.getByRole('button', {
        name: 'conversation.userMessage.expandMessage',
      });
      expect(chevronButton).toBeInTheDocument();
    });

    it('does not render chevron when message is 5 lines or less', () => {
      const shortContent = 'Line 1\nLine 2\nLine 3\nLine 4\nLine 5';
      const { container } = render(<UserMessage content={shortContent} />);
      const chevronButton = container.querySelector(
        'button[aria-label*="conversation.userMessage.expand"]'
      );
      expect(chevronButton).not.toBeInTheDocument();
    });
  });

  describe('accessibility translations', () => {
    it('uses translation key for expand button aria-label', () => {
      const longContent = 'Line 1\nLine 2\nLine 3\nLine 4\nLine 5\nLine 6';
      render(<UserMessage content={longContent} />);
      const expandButton = screen.getByRole('button', {
        name: 'conversation.userMessage.expandMessage',
      });
      expect(expandButton).toBeInTheDocument();
    });

    it('uses translation key for collapse button aria-label when expanded', () => {
      const longContent =
        'Line 1\nLine 2\nLine 3\nLine 4\nLine 5\nLine 6\nLine 7';
      const { rerender } = render(<UserMessage content={longContent} />);
      // Initial state should be collapsed
      const button = screen.getByRole('button', {
        name: 'conversation.userMessage.expandMessage',
      });
      expect(button).toBeInTheDocument();

      // Click to expand
      act(() => {
        button.click();
      });

      // After click, should show collapse label
      rerender(<UserMessage content={longContent} />);
      const collapseButton = screen.queryByRole('button', {
        name: 'conversation.userMessage.collapseMessage',
      });
      // Button should exist with new label
      expect(collapseButton).toBeInTheDocument();
    });
  });

  describe('executor variant display', () => {
    it('displays executor variant when provided', () => {
      const mockTaskAttempt = createMockTaskAttempt();
      render(
        <UserMessage
          content="Test"
          taskAttempt={mockTaskAttempt}
          executorVariant="PLAN"
        />
      );
      expect(screen.getByText('CLAUDE_CODE / PLAN')).toBeInTheDocument();
    });

    it('displays only executor when variant is null', () => {
      const mockTaskAttempt = createMockTaskAttempt();
      render(
        <UserMessage
          content="Test"
          taskAttempt={mockTaskAttempt}
          executorVariant={null}
        />
      );
      expect(screen.getByText('CLAUDE_CODE')).toBeInTheDocument();
      expect(screen.queryByText('/')).not.toBeInTheDocument();
    });

    it('handles undefined executorVariant', () => {
      const mockTaskAttempt = createMockTaskAttempt();
      render(
        <UserMessage
          content="Test"
          taskAttempt={mockTaskAttempt}
          // executorVariant not provided (undefined)
        />
      );
      expect(screen.getByText('CLAUDE_CODE')).toBeInTheDocument();
      expect(screen.queryByText('/')).not.toBeInTheDocument();
    });

    it('handles empty string variant', () => {
      const mockTaskAttempt = createMockTaskAttempt();
      render(
        <UserMessage
          content="Test"
          taskAttempt={mockTaskAttempt}
          executorVariant=""
        />
      );
      // Empty string is falsy, should not show slash
      expect(screen.getByText('CLAUDE_CODE')).toBeInTheDocument();
    });
  });
});
