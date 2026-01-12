import { describe, it, expect, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import UserMessage from '../UserMessage';

// Mock dependencies
vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string) => {
      const translations: Record<string, string> = {
        'conversation.injectedLabel': '(injected)',
        'conversation.injectedTooltip': 'This message was injected into the running process',
      };
      return translations[key] || key;
    },
  }),
}));

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
  useRetryUi: () => ({ activeRetryProcessId: null, isProcessGreyed: () => false }),
}));

describe('UserMessage', () => {
  describe('injected indicator', () => {
    it('renders injected indicator when metadata.injected is true', () => {
      render(
        <UserMessage
          content="Test message"
          metadata={{ injected: true }}
        />
      );
      expect(screen.getByText('(injected)')).toBeInTheDocument();
    });

    it('does not render injected indicator when metadata is null', () => {
      render(
        <UserMessage
          content="Test message"
          metadata={null}
        />
      );
      expect(screen.queryByText('(injected)')).not.toBeInTheDocument();
    });

    it('does not render injected indicator when metadata is undefined', () => {
      render(
        <UserMessage
          content="Test message"
        />
      );
      expect(screen.queryByText('(injected)')).not.toBeInTheDocument();
    });

    it('does not render injected indicator when metadata.injected is false', () => {
      render(
        <UserMessage
          content="Test message"
          metadata={{ injected: false }}
        />
      );
      expect(screen.queryByText('(injected)')).not.toBeInTheDocument();
    });

    it('has aria-label for accessibility', () => {
      render(
        <UserMessage
          content="Test message"
          metadata={{ injected: true }}
        />
      );
      const indicator = screen.getByText('(injected)');
      expect(indicator).toHaveAttribute('aria-label', '(injected)');
    });
  });

  describe('content rendering', () => {
    it('renders message content', () => {
      render(
        <UserMessage
          content="Hello world"
          metadata={null}
        />
      );
      expect(screen.getByText('Hello world')).toBeInTheDocument();
    });
  });
});
