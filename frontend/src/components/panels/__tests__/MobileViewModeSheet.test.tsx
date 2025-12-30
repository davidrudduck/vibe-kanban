import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { MobileViewModeSheet } from '../MobileViewModeSheet';
import type { LayoutMode } from '@/components/layout/TasksLayout';

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
          'mobileViewModeSheet.title': 'View Mode',
          'attemptHeaderActions.preview': 'Preview',
          'attemptHeaderActions.diffs': 'Diffs',
          'attemptHeaderActions.files': 'Files',
          'attemptHeaderActions.terminal': 'Terminal',
          'attemptHeaderActions.processes': 'Processes',
          'mobileViewModeSheet.logs': 'Logs',
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

// Mock useMediaQuery to simulate mobile viewport
vi.mock('@/hooks/useMediaQuery', () => ({
  useMediaQuery: () => false, // Return false for (min-width: 768px) = mobile
}));

describe('MobileViewModeSheet', () => {
  const mockOnModeChange = vi.fn();
  const mockOnOpenChange = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
  });

  const defaultProps = {
    open: true,
    onOpenChange: mockOnOpenChange,
    mode: null as LayoutMode,
    onModeChange: mockOnModeChange,
  };

  it('should render all view mode options when open', () => {
    render(<MobileViewModeSheet {...defaultProps} />);

    expect(screen.getByText('Logs')).toBeInTheDocument();
    expect(screen.getByText('Preview')).toBeInTheDocument();
    expect(screen.getByText('Diffs')).toBeInTheDocument();
    expect(screen.getByText('Files')).toBeInTheDocument();
    expect(screen.getByText('Terminal')).toBeInTheDocument();
    expect(screen.getByText('Processes')).toBeInTheDocument();
  });

  it('should call onModeChange with null when Logs is selected', () => {
    render(<MobileViewModeSheet {...defaultProps} />);

    const logsBtn = screen.getByRole('button', { name: /logs/i });
    fireEvent.click(logsBtn);

    expect(mockOnModeChange).toHaveBeenCalledWith(null);
  });

  it('should call onModeChange with "preview" when Preview is selected', () => {
    render(<MobileViewModeSheet {...defaultProps} />);

    const previewBtn = screen.getByRole('button', { name: /preview/i });
    fireEvent.click(previewBtn);

    expect(mockOnModeChange).toHaveBeenCalledWith('preview');
  });

  it('should call onModeChange with "diffs" when Diffs is selected', () => {
    render(<MobileViewModeSheet {...defaultProps} />);

    const diffsBtn = screen.getByRole('button', { name: /diffs/i });
    fireEvent.click(diffsBtn);

    expect(mockOnModeChange).toHaveBeenCalledWith('diffs');
  });

  it('should call onModeChange with "files" when Files is selected', () => {
    render(<MobileViewModeSheet {...defaultProps} />);

    const filesBtn = screen.getByRole('button', { name: /files/i });
    fireEvent.click(filesBtn);

    expect(mockOnModeChange).toHaveBeenCalledWith('files');
  });

  it('should call onModeChange with "terminal" when Terminal is selected', () => {
    render(<MobileViewModeSheet {...defaultProps} />);

    const terminalBtn = screen.getByRole('button', { name: /terminal/i });
    fireEvent.click(terminalBtn);

    expect(mockOnModeChange).toHaveBeenCalledWith('terminal');
  });

  it('should call onModeChange with "processes" when Processes is selected', () => {
    render(<MobileViewModeSheet {...defaultProps} />);

    const processesBtn = screen.getByRole('button', { name: /processes/i });
    fireEvent.click(processesBtn);

    expect(mockOnModeChange).toHaveBeenCalledWith('processes');
  });

  it('should highlight current mode with accent styling', () => {
    render(<MobileViewModeSheet {...defaultProps} mode="diffs" />);

    const diffsBtn = screen.getByRole('button', { name: /diffs/i });
    // Should have active/selected styling (bg-accent class)
    expect(diffsBtn.className).toContain('bg-accent');
  });

  it('should close sheet after mode selection', () => {
    render(<MobileViewModeSheet {...defaultProps} />);

    const previewBtn = screen.getByRole('button', { name: /preview/i });
    fireEvent.click(previewBtn);

    expect(mockOnOpenChange).toHaveBeenCalledWith(false);
  });

  it('should have minimum 48px touch targets for mode buttons', () => {
    render(<MobileViewModeSheet {...defaultProps} />);

    // Get mode selection buttons (not the title or other elements)
    const logsBtn = screen.getByRole('button', { name: /logs/i });
    // Each mode button should have h-12 (48px) class
    expect(logsBtn.className).toContain('h-12');
  });
});
