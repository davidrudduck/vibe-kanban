import '@testing-library/jest-dom/vitest';
import { afterEach, describe, it, expect, vi } from 'vitest';
import { cleanup, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { I18nextProvider } from 'react-i18next';
import i18n from 'i18next';
import { ConversationNavOverlay } from './ConversationNavOverlay';

// vitest globals are disabled in this workspace, so testing-library's
// auto-cleanup afterEach hook is not registered; do it manually.
afterEach(cleanup);

// Minimal i18n test instance.
i18n.init({
  lng: 'en',
  resources: {
    en: {
      common: {
        workspaces: {
          nav: {
            goToTop: 'Go to top',
            previousUserMessage: 'Previous user message',
            nextUserMessage: 'Next user message',
            scrollToBottom: 'Scroll to bottom',
          },
        },
      },
    },
  },
  ns: ['common'],
  defaultNS: 'common',
});

const baseProps = {
  isAtTop: false,
  isAtBottom: false,
  hasPreviousUserMessage: true,
  hasNextUserMessage: true,
  onScrollToTop: vi.fn(),
  onScrollToPreviousMessage: vi.fn(),
  onScrollToNextMessage: vi.fn(),
  onScrollToBottom: vi.fn(),
};

function renderWithI18n(ui: React.ReactElement) {
  return render(<I18nextProvider i18n={i18n}>{ui}</I18nextProvider>);
}

describe('ConversationNavOverlay', () => {
  it('renders all four buttons in the middle of a long conversation', () => {
    renderWithI18n(<ConversationNavOverlay {...baseProps} />);
    expect(
      screen.getByRole('button', { name: 'Go to top' })
    ).toBeInTheDocument();
    expect(
      screen.getByRole('button', { name: 'Previous user message' })
    ).toBeInTheDocument();
    expect(
      screen.getByRole('button', { name: 'Next user message' })
    ).toBeInTheDocument();
    expect(
      screen.getByRole('button', { name: 'Scroll to bottom' })
    ).toBeInTheDocument();
  });

  it('hides the entire overlay when both edges are reached', () => {
    const { container } = renderWithI18n(
      <ConversationNavOverlay {...baseProps} isAtTop isAtBottom />
    );
    expect(container).toBeEmptyDOMElement();
  });

  it('hides "previous user message" when none exists', () => {
    renderWithI18n(
      <ConversationNavOverlay {...baseProps} hasPreviousUserMessage={false} />
    );
    expect(
      screen.queryByRole('button', { name: 'Previous user message' })
    ).toBeNull();
  });

  it('hides "next user message" when none exists', () => {
    renderWithI18n(
      <ConversationNavOverlay {...baseProps} hasNextUserMessage={false} />
    );
    expect(
      screen.queryByRole('button', { name: 'Next user message' })
    ).toBeNull();
  });

  it('renders nothing on narrow viewports', () => {
    const { container } = renderWithI18n(
      <ConversationNavOverlay {...baseProps} isNarrow />
    );
    expect(container).toBeEmptyDOMElement();
  });

  it('invokes the correct callback on click', async () => {
    const onScrollToTop = vi.fn();
    renderWithI18n(
      <ConversationNavOverlay {...baseProps} onScrollToTop={onScrollToTop} />
    );
    await userEvent.click(screen.getByRole('button', { name: 'Go to top' }));
    expect(onScrollToTop).toHaveBeenCalledTimes(1);
  });
});
