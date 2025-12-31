import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import MobileAllProjectsKanban, {
  type AllProjectsKanbanColumns,
} from '../MobileAllProjectsKanban';
import type { TaskWithProjectInfo, TaskStatus } from 'shared/types';

// Mock the useSwipe hook
vi.mock('@/hooks/useSwipe', () => ({
  useSwipe: (handlers: {
    onSwipeLeft?: () => void;
    onSwipeRight?: () => void;
  }) => ({
    onTouchStart: vi.fn(),
    onTouchEnd: vi.fn(),
    // Expose handlers for testing
    _handlers: handlers,
  }),
}));

const createMockTask = (
  id: string,
  status: TaskStatus,
  projectName: string
): TaskWithProjectInfo => ({
  id,
  project_id: `project-${id}`,
  title: `Task ${id}`,
  description: `Description for task ${id}`,
  status,
  parent_task_id: null,
  swarm_task_id: null,
  created_at: '2024-01-01T00:00:00Z',
  updated_at: '2024-01-01T00:00:00Z',
  is_remote: false,
  remote_assignee_user_id: null,
  remote_assignee_name: null,
  remote_assignee_username: null,
  remote_version: BigInt(0),
  remote_last_synced_at: null,
  remote_stream_node_id: null,
  remote_stream_url: null,
  archived_at: null,
  activity_at: null,
  assignee_first_name: null,
  assignee_last_name: null,
  assignee_username: null,
  has_in_progress_attempt: false,
  has_merged_attempt: false,
  last_attempt_failed: false,
  executor: 'claude',
  project_name: projectName,
  source_node_name: null,
});

const createMockColumns = (): AllProjectsKanbanColumns => ({
  todo: [
    createMockTask('1', 'todo', 'Project A'),
    createMockTask('2', 'todo', 'Project B'),
  ],
  inprogress: [createMockTask('3', 'inprogress', 'Project A')],
  inreview: [createMockTask('4', 'inreview', 'Project C')],
  done: [],
  cancelled: [],
});

describe('MobileAllProjectsKanban', () => {
  const mockOnViewTaskDetails = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('should render only one column at a time', () => {
    const columns = createMockColumns();
    render(
      <MobileAllProjectsKanban
        columns={columns}
        onViewTaskDetails={mockOnViewTaskDetails}
      />
    );

    // Check that the board is rendered
    expect(
      screen.getByTestId('mobile-all-projects-kanban')
    ).toBeInTheDocument();

    // First column (todo) should be visible
    expect(screen.getByTestId('column-todo')).toBeInTheDocument();
    expect(screen.getByTestId('column-todo')).toHaveAttribute(
      'aria-hidden',
      'false'
    );

    // Other columns should be hidden
    expect(screen.getByTestId('column-inprogress')).toHaveAttribute(
      'aria-hidden',
      'true'
    );
  });

  it('should show column indicator dots for all columns', () => {
    const columns = createMockColumns();
    render(
      <MobileAllProjectsKanban
        columns={columns}
        onViewTaskDetails={mockOnViewTaskDetails}
      />
    );

    // Should have 5 indicator dots (rendered as tabs in tablist)
    const dots = screen.getAllByRole('tab');
    expect(dots).toHaveLength(5);
  });

  it('should navigate to next column on next arrow click', () => {
    const columns = createMockColumns();
    render(
      <MobileAllProjectsKanban
        columns={columns}
        onViewTaskDetails={mockOnViewTaskDetails}
      />
    );

    // Initially on "To Do" column
    expect(screen.getByText(/To Do/)).toBeInTheDocument();

    // Click next button
    const nextButton = screen.getByRole('button', { name: /next column/i });
    fireEvent.click(nextButton);

    // Now on "In Progress" column
    expect(screen.getByText(/In Progress/)).toBeInTheDocument();
    expect(screen.getByTestId('column-inprogress')).toHaveAttribute(
      'aria-hidden',
      'false'
    );
  });

  it('should navigate to previous column on prev arrow click', () => {
    const columns = createMockColumns();
    render(
      <MobileAllProjectsKanban
        columns={columns}
        onViewTaskDetails={mockOnViewTaskDetails}
      />
    );

    // First go to next column
    const nextButton = screen.getByRole('button', { name: /next column/i });
    fireEvent.click(nextButton);

    // Now click previous
    const prevButton = screen.getByRole('button', { name: /previous column/i });
    fireEvent.click(prevButton);

    // Back on "To Do" column
    expect(screen.getByTestId('column-todo')).toHaveAttribute(
      'aria-hidden',
      'false'
    );
  });

  it('should not navigate past first column', () => {
    const columns = createMockColumns();
    render(
      <MobileAllProjectsKanban
        columns={columns}
        onViewTaskDetails={mockOnViewTaskDetails}
      />
    );

    // Initially on first column, prev button should be disabled
    const prevButton = screen.getByRole('button', { name: /previous column/i });
    expect(prevButton).toBeDisabled();
  });

  it('should not navigate past last column', () => {
    const columns = createMockColumns();
    render(
      <MobileAllProjectsKanban
        columns={columns}
        onViewTaskDetails={mockOnViewTaskDetails}
      />
    );

    // Navigate to the last column
    const nextButton = screen.getByRole('button', { name: /next column/i });
    for (let i = 0; i < 5; i++) {
      fireEvent.click(nextButton);
    }

    // Next button should be disabled on last column
    expect(nextButton).toBeDisabled();
  });

  it('should show current column name and task count', () => {
    const columns = createMockColumns();
    render(
      <MobileAllProjectsKanban
        columns={columns}
        onViewTaskDetails={mockOnViewTaskDetails}
      />
    );

    // Should show "To Do" with count 2
    expect(screen.getByText('To Do')).toBeInTheDocument();
    expect(screen.getByText('(2)')).toBeInTheDocument();
  });

  it('should display empty state when column has no tasks', () => {
    const columns = createMockColumns();
    render(
      <MobileAllProjectsKanban
        columns={columns}
        onViewTaskDetails={mockOnViewTaskDetails}
      />
    );

    // Navigate to "Done" column (which is empty)
    const nextButton = screen.getByRole('button', { name: /next column/i });
    fireEvent.click(nextButton); // In Progress
    fireEvent.click(nextButton); // In Review
    fireEvent.click(nextButton); // Done

    // Should show empty state
    expect(screen.getAllByTestId('empty-column').length).toBeGreaterThan(0);
  });

  it('should render task cards with project badges', () => {
    const columns = createMockColumns();
    render(
      <MobileAllProjectsKanban
        columns={columns}
        onViewTaskDetails={mockOnViewTaskDetails}
      />
    );

    // Should show project names on cards (use getAllByText since same project can appear multiple times)
    expect(screen.getAllByText('Project A').length).toBeGreaterThan(0);
    expect(screen.getAllByText('Project B').length).toBeGreaterThan(0);
  });

  it('should call onViewTaskDetails when task card is clicked', () => {
    const columns = createMockColumns();
    render(
      <MobileAllProjectsKanban
        columns={columns}
        onViewTaskDetails={mockOnViewTaskDetails}
      />
    );

    // Click on a task card
    const taskCard = screen.getByText('Task 1');
    fireEvent.click(taskCard);

    expect(mockOnViewTaskDetails).toHaveBeenCalledWith(columns.todo[0]);
  });
});
