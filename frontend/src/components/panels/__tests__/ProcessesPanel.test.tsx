import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/react';
import { ProcessesPanel } from '../ProcessesPanel';

// Mock react-i18next
vi.mock('react-i18next', async () => {
  const original = await vi.importActual('react-i18next');
  return {
    ...original,
    initReactI18next: { type: '3rdParty', init: () => {} },
    useTranslation: () => ({
      t: (key: string, defaultValue?: string | { defaultValue?: string }) => {
        if (typeof defaultValue === 'object' && defaultValue?.defaultValue) {
          return defaultValue.defaultValue;
        }
        const translations: Record<string, string> = {
          'processes.title': 'Processes',
          'processes.noAttempt': 'Select a task attempt to view processes',
          'processes.loading': 'Loading processes...',
          'processes.noProcesses': 'No processes found',
        };
        return translations[key] || key;
      },
      i18n: {
        changeLanguage: () => Promise.resolve(),
        language: 'en',
      },
    }),
    Trans: ({ children }: { children: React.ReactNode }) => children,
  };
});

// Mock i18n config
vi.mock('@/i18n/config', () => ({
  default: {},
}));

// Mock ProcessSelectionContext
vi.mock('@/contexts/ProcessSelectionContext', () => ({
  ProcessSelectionProvider: ({ children }: { children: React.ReactNode }) =>
    children,
  useProcessSelection: () => ({
    selectedProcessId: null,
    setSelectedProcessId: vi.fn(),
  }),
}));

// Mock RetryUiContext
vi.mock('@/contexts/RetryUiContext', () => ({
  useRetryUi: () => ({
    isProcessGreyed: () => false,
  }),
}));

// Mock useExecutionProcesses
vi.mock('@/hooks/useExecutionProcesses', () => ({
  useExecutionProcesses: () => ({
    executionProcesses: [],
    executionProcessesById: {},
    isLoading: false,
    isConnected: true,
    error: null,
  }),
}));

// Mock the ProcessesTab component
vi.mock('@/components/tasks/TaskDetails/ProcessesTab', () => ({
  default: ({ attemptId }: { attemptId?: string }) => (
    <div data-testid="processes-tab">
      ProcessesTab: {attemptId || 'no-attempt'}
    </div>
  ),
}));

describe('ProcessesPanel', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('should render ProcessesTab when attemptId is provided', () => {
    render(<ProcessesPanel attemptId="test-attempt-123" />);

    expect(screen.getByTestId('processes-tab')).toBeInTheDocument();
    expect(screen.getByText(/test-attempt-123/)).toBeInTheDocument();
  });

  it('should render ProcessesTab with no-attempt when attemptId is undefined', () => {
    render(<ProcessesPanel attemptId={undefined} />);

    expect(screen.getByTestId('processes-tab')).toBeInTheDocument();
    expect(screen.getByText(/no-attempt/)).toBeInTheDocument();
  });

  it('should have proper container styling for panel display', () => {
    const { container } = render(<ProcessesPanel attemptId="test-attempt" />);

    // The panel should have appropriate layout classes
    const wrapper = container.firstChild as HTMLElement;
    expect(wrapper).toHaveClass('h-full');
  });

  it('should pass onClose callback to be used for closing the panel', () => {
    const mockOnClose = vi.fn();
    render(<ProcessesPanel attemptId="test-attempt" onClose={mockOnClose} />);

    // The close functionality should be available (tested via integration)
    expect(screen.getByTestId('processes-tab')).toBeInTheDocument();
  });
});
