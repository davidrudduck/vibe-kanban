import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import { CompactLabelList } from '../CompactLabelList';
import type { Label } from 'shared/types';

// Create mock labels
function createMockLabel(id: string, name: string): Label {
  return {
    id,
    name,
    color: '#4B5563',
    icon: 'tag',
    project_id: 'project-1',
    created_at: new Date().toISOString(),
    updated_at: new Date().toISOString(),
    version: BigInt(1),
  };
}

describe('CompactLabelList', () => {
  it('returns null for undefined labels', () => {
    const { container } = render(<CompactLabelList labels={undefined} />);
    expect(container.firstChild).toBeNull();
  });

  it('returns null for empty labels array', () => {
    const { container } = render(<CompactLabelList labels={[]} />);
    expect(container.firstChild).toBeNull();
  });

  it('shows all labels when count <= maxVisible', () => {
    const labels = [
      createMockLabel('1', 'Bug'),
      createMockLabel('2', 'Feature'),
    ];
    render(<CompactLabelList labels={labels} maxVisible={2} />);

    expect(screen.getByText('Bug')).toBeInTheDocument();
    expect(screen.getByText('Feature')).toBeInTheDocument();
    expect(screen.queryByText(/\+/)).not.toBeInTheDocument();
  });

  it('shows all labels when count equals maxVisible', () => {
    const labels = [
      createMockLabel('1', 'Bug'),
      createMockLabel('2', 'Feature'),
      createMockLabel('3', 'High Priority'),
    ];
    render(<CompactLabelList labels={labels} maxVisible={3} />);

    expect(screen.getByText('Bug')).toBeInTheDocument();
    expect(screen.getByText('Feature')).toBeInTheDocument();
    expect(screen.getByText('High Priority')).toBeInTheDocument();
    expect(screen.queryByText(/\+/)).not.toBeInTheDocument();
  });

  it('shows "+N" overflow badge when count > maxVisible', () => {
    const labels = [
      createMockLabel('1', 'Bug'),
      createMockLabel('2', 'Feature'),
      createMockLabel('3', 'High Priority'),
      createMockLabel('4', 'Documentation'),
    ];
    render(<CompactLabelList labels={labels} maxVisible={2} />);

    expect(screen.getByText('Bug')).toBeInTheDocument();
    expect(screen.getByText('Feature')).toBeInTheDocument();
    expect(screen.getByText('+2')).toBeInTheDocument();
    expect(screen.queryByText('High Priority')).not.toBeInTheDocument();
    expect(screen.queryByText('Documentation')).not.toBeInTheDocument();
  });

  it('overflow badge has tooltip trigger functionality', () => {
    const labels = [
      createMockLabel('1', 'Bug'),
      createMockLabel('2', 'Feature'),
      createMockLabel('3', 'High Priority'),
      createMockLabel('4', 'Documentation'),
    ];
    render(<CompactLabelList labels={labels} maxVisible={2} />);

    // The overflow badge should have data-state attribute from Radix tooltip
    const overflowBadge = screen.getByText('+2');
    expect(overflowBadge).toHaveAttribute('data-state');
  });

  it('defaults maxVisible to 2', () => {
    const labels = [
      createMockLabel('1', 'Bug'),
      createMockLabel('2', 'Feature'),
      createMockLabel('3', 'High Priority'),
    ];
    render(<CompactLabelList labels={labels} />);

    expect(screen.getByText('Bug')).toBeInTheDocument();
    expect(screen.getByText('Feature')).toBeInTheDocument();
    expect(screen.getByText('+1')).toBeInTheDocument();
  });

  it('respects custom maxVisible value', () => {
    const labels = [
      createMockLabel('1', 'Bug'),
      createMockLabel('2', 'Feature'),
      createMockLabel('3', 'High Priority'),
      createMockLabel('4', 'Documentation'),
      createMockLabel('5', 'Enhancement'),
    ];
    render(<CompactLabelList labels={labels} maxVisible={3} />);

    expect(screen.getByText('Bug')).toBeInTheDocument();
    expect(screen.getByText('Feature')).toBeInTheDocument();
    expect(screen.getByText('High Priority')).toBeInTheDocument();
    expect(screen.getByText('+2')).toBeInTheDocument();
  });

  it('applies custom className', () => {
    const labels = [createMockLabel('1', 'Bug')];
    const { container } = render(
      <CompactLabelList labels={labels} className="custom-class" />
    );

    expect(container.firstChild).toHaveClass('custom-class');
  });

  it('uses sm size by default', () => {
    const labels = [createMockLabel('1', 'Bug')];
    render(<CompactLabelList labels={labels} />);

    const labelBadge = screen.getByText('Bug');
    // sm size has text-xs class on the parent span (the label badge container)
    expect(labelBadge.closest('span.rounded-full')).toHaveClass('text-xs');
  });

  it('respects md size prop', () => {
    const labels = [createMockLabel('1', 'Bug')];
    render(<CompactLabelList labels={labels} size="md" />);

    const labelBadge = screen.getByText('Bug');
    // md size has text-sm class on the parent span (the label badge container)
    expect(labelBadge.closest('span.rounded-full')).toHaveClass('text-sm');
  });
});
