import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import MobileColumnHeader from '../MobileColumnHeader';

describe('MobileColumnHeader', () => {
  const mockOnPrev = vi.fn();
  const mockOnNext = vi.fn();

  const defaultProps = {
    name: 'To Do',
    count: 5,
    color: '--status-todo',
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
    expect(prevButton).not.toBeDisabled();
  });

  it('should disable left arrow when on first column', () => {
    render(
      <MobileColumnHeader {...defaultProps} isFirst={true} currentIndex={0} />
    );
    const prevButton = screen.getByRole('button', {
      name: /previous column/i,
    });
    expect(prevButton).toBeDisabled();
  });

  it('should show right arrow when not on last column', () => {
    render(<MobileColumnHeader {...defaultProps} isLast={false} />);
    const nextButton = screen.getByRole('button', { name: /next column/i });
    expect(nextButton).not.toBeDisabled();
  });

  it('should disable right arrow when on last column', () => {
    render(
      <MobileColumnHeader {...defaultProps} isLast={true} currentIndex={4} />
    );
    const nextButton = screen.getByRole('button', { name: /next column/i });
    expect(nextButton).toBeDisabled();
  });

  it('should call onPrev when left arrow clicked', () => {
    render(<MobileColumnHeader {...defaultProps} />);
    const prevButton = screen.getByRole('button', {
      name: /previous column/i,
    });
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
    // Dots are rendered as tabs in the tablist
    const dots = screen.getAllByRole('tab');
    expect(dots).toHaveLength(5);
  });

  it('should highlight the current indicator dot', () => {
    render(
      <MobileColumnHeader {...defaultProps} currentIndex={2} totalColumns={5} />
    );
    const dots = screen.getAllByRole('tab');
    // The third dot (index 2) should be selected
    expect(dots[2]).toHaveAttribute('aria-selected', 'true');
    expect(dots[2]).toHaveClass('bg-foreground');
    // Other dots should not be selected
    expect(dots[0]).toHaveAttribute('aria-selected', 'false');
    expect(dots[1]).toHaveAttribute('aria-selected', 'false');
    expect(dots[3]).toHaveAttribute('aria-selected', 'false');
    expect(dots[4]).toHaveAttribute('aria-selected', 'false');
  });

  it('should display the color indicator dot', () => {
    render(<MobileColumnHeader {...defaultProps} color="--status-todo" />);
    // The color indicator is a sibling to the column name text
    const colorIndicator = screen.getByText('To Do').previousElementSibling;
    expect(colorIndicator).toHaveClass('rounded-full');
    expect(colorIndicator).toHaveClass('h-3');
    expect(colorIndicator).toHaveClass('w-3');
  });
});
