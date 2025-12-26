import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import MobileColumnHeader from '../MobileColumnHeader';

describe('MobileColumnHeader', () => {
  const mockOnPrev = vi.fn();
  const mockOnNext = vi.fn();

  const defaultProps = {
    name: 'To Do',
    count: 5,
    color: '#3b82f6',
    isFirst: false,
    isLast: false,
    onPrev: mockOnPrev,
    onNext: mockOnNext,
    currentIndex: 1,
    totalColumns: 5,
  };

  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('should display column name', () => {
    render(<MobileColumnHeader {...defaultProps} />);
    expect(screen.getByText('To Do')).toBeInTheDocument();
  });

  it('should display task count', () => {
    render(<MobileColumnHeader {...defaultProps} />);
    expect(screen.getByText('(5)')).toBeInTheDocument();
  });

  it('should show left arrow when not on first column', () => {
    render(<MobileColumnHeader {...defaultProps} isFirst={false} />);
    const prevButton = screen.getByRole('button', { name: /previous column/i });
    expect(prevButton).not.toHaveClass('invisible');
  });

  it('should hide left arrow when on first column', () => {
    render(<MobileColumnHeader {...defaultProps} isFirst={true} currentIndex={0} />);
    const prevButton = screen.getByRole('button', { name: /previous column/i });
    expect(prevButton).toHaveClass('invisible');
  });

  it('should show right arrow when not on last column', () => {
    render(<MobileColumnHeader {...defaultProps} isLast={false} />);
    const nextButton = screen.getByRole('button', { name: /next column/i });
    expect(nextButton).not.toHaveClass('invisible');
  });

  it('should hide right arrow when on last column', () => {
    render(<MobileColumnHeader {...defaultProps} isLast={true} currentIndex={4} />);
    const nextButton = screen.getByRole('button', { name: /next column/i });
    expect(nextButton).toHaveClass('invisible');
  });

  it('should call onPrev when left arrow clicked', () => {
    render(<MobileColumnHeader {...defaultProps} />);
    const prevButton = screen.getByRole('button', { name: /previous column/i });
    fireEvent.click(prevButton);
    expect(mockOnPrev).toHaveBeenCalledTimes(1);
  });

  it('should call onNext when right arrow clicked', () => {
    render(<MobileColumnHeader {...defaultProps} />);
    const nextButton = screen.getByRole('button', { name: /next column/i });
    fireEvent.click(nextButton);
    expect(mockOnNext).toHaveBeenCalledTimes(1);
  });

  it('should render indicator dots for all columns', () => {
    render(<MobileColumnHeader {...defaultProps} totalColumns={5} />);
    const dots = screen.getAllByTestId('column-indicator-dot');
    expect(dots).toHaveLength(5);
  });

  it('should highlight the current indicator dot', () => {
    render(<MobileColumnHeader {...defaultProps} currentIndex={2} totalColumns={5} />);
    const dots = screen.getAllByTestId('column-indicator-dot');
    // The third dot (index 2) should have the primary color
    expect(dots[2]).toHaveClass('bg-primary');
    // Other dots should have the muted color
    expect(dots[0]).toHaveClass('bg-muted-foreground/30');
    expect(dots[1]).toHaveClass('bg-muted-foreground/30');
    expect(dots[3]).toHaveClass('bg-muted-foreground/30');
    expect(dots[4]).toHaveClass('bg-muted-foreground/30');
  });

  it('should display the color indicator', () => {
    render(<MobileColumnHeader {...defaultProps} color="#ef4444" />);
    // Look for the color indicator span
    const colorIndicator = screen.getByText('To Do').previousElementSibling;
    expect(colorIndicator).toHaveStyle({ backgroundColor: '#ef4444' });
  });
});
