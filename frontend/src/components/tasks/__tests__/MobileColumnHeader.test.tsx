import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import MobileColumnHeader from '../MobileColumnHeader';

describe('MobileColumnHeader', () => {
  const defaultProps = {
    name: 'To Do',
    count: 5,
    color: '--neutral-foreground',
    isFirst: false,
    isLast: false,
    onPrev: vi.fn(),
    onNext: vi.fn(),
    currentIndex: 1,
    totalColumns: 5,
  };

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
    const prevBtn = screen.getByTestId('prev-column-btn');
    expect(prevBtn).not.toBeDisabled();
  });

  it('should disable left arrow when on first column', () => {
    render(<MobileColumnHeader {...defaultProps} isFirst={true} />);
    const prevBtn = screen.getByTestId('prev-column-btn');
    expect(prevBtn).toBeDisabled();
  });

  it('should show right arrow when not on last column', () => {
    render(<MobileColumnHeader {...defaultProps} isLast={false} />);
    const nextBtn = screen.getByTestId('next-column-btn');
    expect(nextBtn).not.toBeDisabled();
  });

  it('should disable right arrow when on last column', () => {
    render(<MobileColumnHeader {...defaultProps} isLast={true} />);
    const nextBtn = screen.getByTestId('next-column-btn');
    expect(nextBtn).toBeDisabled();
  });

  it('should call onPrev when left arrow clicked', () => {
    const onPrev = vi.fn();
    render(<MobileColumnHeader {...defaultProps} onPrev={onPrev} />);
    const prevBtn = screen.getByTestId('prev-column-btn');
    fireEvent.click(prevBtn);
    expect(onPrev).toHaveBeenCalledTimes(1);
  });

  it('should call onNext when right arrow clicked', () => {
    const onNext = vi.fn();
    render(<MobileColumnHeader {...defaultProps} onNext={onNext} />);
    const nextBtn = screen.getByTestId('next-column-btn');
    fireEvent.click(nextBtn);
    expect(onNext).toHaveBeenCalledTimes(1);
  });

  it('should display indicator dots for all columns', () => {
    render(<MobileColumnHeader {...defaultProps} totalColumns={5} />);
    const dots = screen.getAllByRole('tab');
    expect(dots).toHaveLength(5);
  });

  it('should highlight current column indicator', () => {
    render(
      <MobileColumnHeader {...defaultProps} currentIndex={2} totalColumns={5} />
    );
    const dots = screen.getAllByRole('tab');
    expect(dots[2]).toHaveAttribute('aria-selected', 'true');
    expect(dots[0]).toHaveAttribute('aria-selected', 'false');
    expect(dots[1]).toHaveAttribute('aria-selected', 'false');
    expect(dots[3]).toHaveAttribute('aria-selected', 'false');
    expect(dots[4]).toHaveAttribute('aria-selected', 'false');
  });

  it('should not call onPrev when left arrow is disabled', () => {
    const onPrev = vi.fn();
    render(
      <MobileColumnHeader {...defaultProps} isFirst={true} onPrev={onPrev} />
    );
    const prevBtn = screen.getByTestId('prev-column-btn');
    fireEvent.click(prevBtn);
    expect(onPrev).not.toHaveBeenCalled();
  });

  it('should not call onNext when right arrow is disabled', () => {
    const onNext = vi.fn();
    render(
      <MobileColumnHeader {...defaultProps} isLast={true} onNext={onNext} />
    );
    const nextBtn = screen.getByTestId('next-column-btn');
    fireEvent.click(nextBtn);
    expect(onNext).not.toHaveBeenCalled();
  });
});
