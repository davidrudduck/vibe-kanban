import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { MobileDetailHeader } from '../MobileDetailHeader';

// Mock react-i18next
vi.mock('react-i18next', async () => {
  const original = await vi.importActual('react-i18next');
  return {
    ...original,
    initReactI18next: { type: '3rdParty', init: () => {} },
    useTranslation: () => ({
      t: (key: string, defaultValue?: { defaultValue?: string }) => {
        if (typeof defaultValue === 'object' && defaultValue?.defaultValue) {
          return defaultValue.defaultValue;
        }
        const translations: Record<string, string> = {
          'common:buttons.back': 'Back',
          'mobileDetailHeader.viewMode': 'View mode',
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

describe('MobileDetailHeader', () => {
  const mockOnBack = vi.fn();
  const mockOnViewModePress = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
  });

  const defaultProps = {
    title: 'Test Task',
    onBack: mockOnBack,
  };

  it('should render with task title', () => {
    render(<MobileDetailHeader {...defaultProps} />);
    expect(screen.getByText('Test Task')).toBeInTheDocument();
  });

  it('should render back button', () => {
    render(<MobileDetailHeader {...defaultProps} />);
    const backBtn = screen.getByRole('button', { name: /back/i });
    expect(backBtn).toBeInTheDocument();
  });

  it('should call onBack when back button is clicked', () => {
    render(<MobileDetailHeader {...defaultProps} />);
    const backBtn = screen.getByRole('button', { name: /back/i });
    fireEvent.click(backBtn);
    expect(mockOnBack).toHaveBeenCalledTimes(1);
  });

  it('should have minimum 44px touch target for back button', () => {
    render(<MobileDetailHeader {...defaultProps} />);
    const backBtn = screen.getByRole('button', { name: /back/i });
    // Check button has appropriate size class (h-11 = 44px)
    expect(backBtn.className).toMatch(/h-11/);
  });

  it('should render view mode button when onViewModePress is provided', () => {
    render(
      <MobileDetailHeader
        {...defaultProps}
        onViewModePress={mockOnViewModePress}
      />
    );
    const modeBtn = screen.getByRole('button', { name: /view mode/i });
    expect(modeBtn).toBeInTheDocument();
  });

  it('should not render view mode button when onViewModePress is not provided', () => {
    render(<MobileDetailHeader {...defaultProps} />);
    expect(
      screen.queryByRole('button', { name: /view mode/i })
    ).not.toBeInTheDocument();
  });

  it('should call onViewModePress when view mode button is clicked', () => {
    render(
      <MobileDetailHeader
        {...defaultProps}
        onViewModePress={mockOnViewModePress}
      />
    );
    const modeBtn = screen.getByRole('button', { name: /view mode/i });
    fireEvent.click(modeBtn);
    expect(mockOnViewModePress).toHaveBeenCalledTimes(1);
  });

  it('should truncate long titles with ellipsis', () => {
    const longTitle = 'This is a very long task title that should be truncated';
    render(<MobileDetailHeader {...defaultProps} title={longTitle} />);
    const titleElement = screen.getByText(longTitle);
    expect(titleElement.className).toMatch(/truncate|overflow-hidden/);
  });

  it('should render subtitle when provided', () => {
    render(<MobileDetailHeader {...defaultProps} subtitle="branch-name" />);
    expect(screen.getByText('branch-name')).toBeInTheDocument();
  });

  it('should render actions slot when provided', () => {
    render(
      <MobileDetailHeader
        {...defaultProps}
        actions={<button data-testid="custom-action">Action</button>}
      />
    );
    expect(screen.getByTestId('custom-action')).toBeInTheDocument();
  });
});
