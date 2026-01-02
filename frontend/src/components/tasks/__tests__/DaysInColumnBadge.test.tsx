import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import { DaysInColumnBadge } from '../DaysInColumnBadge';

describe('DaysInColumnBadge', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2025-01-15T12:00:00Z'));
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('returns null for activityAt less than 24 hours ago', () => {
    const activityAt = new Date('2025-01-15T00:00:00Z');
    const { container } = render(<DaysInColumnBadge activityAt={activityAt} />);
    expect(container.firstChild).toBeNull();
  });

  it('returns null for null activityAt', () => {
    const { container } = render(<DaysInColumnBadge activityAt={null} />);
    expect(container.firstChild).toBeNull();
  });

  it('returns null for undefined activityAt', () => {
    const { container } = render(<DaysInColumnBadge activityAt={undefined} />);
    expect(container.firstChild).toBeNull();
  });

  it('renders "1d" for 1 day', () => {
    const activityAt = new Date('2025-01-14T11:00:00Z');
    render(<DaysInColumnBadge activityAt={activityAt} />);
    expect(screen.getByText('1d')).toBeInTheDocument();
  });

  it('renders "3d" with warning style for 3 days', () => {
    const activityAt = new Date('2025-01-12T12:00:00Z');
    render(<DaysInColumnBadge activityAt={activityAt} />);
    const badge = screen.getByText('3d');
    expect(badge).toBeInTheDocument();
    expect(badge).toHaveClass('bg-amber-100');
  });

  it('renders "7d+" with strong warning for 7+ days', () => {
    const activityAt = new Date('2025-01-08T12:00:00Z');
    render(<DaysInColumnBadge activityAt={activityAt} />);
    const badge = screen.getByText('7d+');
    expect(badge).toBeInTheDocument();
    expect(badge).toHaveClass('bg-red-100');
  });

  it('renders "7d+" for 14 days', () => {
    const activityAt = new Date('2025-01-01T12:00:00Z');
    render(<DaysInColumnBadge activityAt={activityAt} />);
    expect(screen.getByText('7d+')).toBeInTheDocument();
  });

  it('handles ISO string input', () => {
    const activityAt = '2025-01-12T12:00:00Z';
    render(<DaysInColumnBadge activityAt={activityAt} />);
    expect(screen.getByText('3d')).toBeInTheDocument();
  });

  it('handles Date object input', () => {
    const activityAt = new Date('2025-01-12T12:00:00Z');
    render(<DaysInColumnBadge activityAt={activityAt} />);
    expect(screen.getByText('3d')).toBeInTheDocument();
  });

  it('applies custom className', () => {
    const activityAt = new Date('2025-01-12T12:00:00Z');
    render(
      <DaysInColumnBadge activityAt={activityAt} className="custom-class" />
    );
    const badge = screen.getByText('3d');
    expect(badge).toHaveClass('custom-class');
  });

  it('has title attribute with full day count', () => {
    const activityAt = new Date('2025-01-12T12:00:00Z');
    render(<DaysInColumnBadge activityAt={activityAt} />);
    const badge = screen.getByText('3d');
    expect(badge).toHaveAttribute('title', '3 days in this column');
  });

  it('has singular "day" in title for 1 day', () => {
    const activityAt = new Date('2025-01-14T11:00:00Z');
    render(<DaysInColumnBadge activityAt={activityAt} />);
    const badge = screen.getByText('1d');
    expect(badge).toHaveAttribute('title', '1 day in this column');
  });
});
