import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { TemplatePicker, TemplatePickerProps } from '../TemplatePicker';

// Mock react-i18next
vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string, fallback?: string) => fallback || key,
    i18n: { changeLanguage: () => Promise.resolve(), language: 'en' },
  }),
}));

// Mock framer-motion to avoid animation issues in tests
vi.mock('framer-motion', () => ({
  motion: {
    div: ({
      children,
      className,
      onClick,
      ...props
    }: React.HTMLAttributes<HTMLDivElement>) => (
      <div className={className} onClick={onClick} {...props}>
        {children}
      </div>
    ),
  },
  AnimatePresence: ({ children }: { children: React.ReactNode }) => (
    <>{children}</>
  ),
}));

// Mock useIsMobile hook - default to desktop mode
vi.mock('@/hooks/useIsMobile', () => ({
  useIsMobile: () => false,
}));

// Base props for tests
const baseProps: TemplatePickerProps = {
  open: true,
  onOpenChange: vi.fn(),
  onSelect: vi.fn(),
};

describe('TemplatePicker', () => {
  describe('Retry Button', () => {
    it('shows retry button when error and onRetry provided', () => {
      const onRetry = vi.fn();
      render(<TemplatePicker {...baseProps} error="Failed" onRetry={onRetry} />);
      expect(screen.getByRole('button', { name: /retry/i })).toBeInTheDocument();
    });

    it('calls onRetry when retry button clicked', () => {
      const onRetry = vi.fn();
      render(<TemplatePicker {...baseProps} error="Failed" onRetry={onRetry} />);
      fireEvent.click(screen.getByRole('button', { name: /retry/i }));
      expect(onRetry).toHaveBeenCalledTimes(1);
    });

    it('hides retry button when no error', () => {
      const onRetry = vi.fn();
      render(<TemplatePicker {...baseProps} onRetry={onRetry} />);
      expect(
        screen.queryByRole('button', { name: /retry/i })
      ).not.toBeInTheDocument();
    });

    it('hides retry button when error but no onRetry callback', () => {
      render(<TemplatePicker {...baseProps} error="Failed" />);
      expect(
        screen.queryByRole('button', { name: /retry/i })
      ).not.toBeInTheDocument();
    });
  });
});
