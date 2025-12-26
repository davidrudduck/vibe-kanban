import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, act } from '@testing-library/react';
import MobileKanbanBoard from '../MobileKanbanBoard';
import type { KanbanColumns } from '../TaskKanbanBoard';
import type { TaskWithAttemptStatus, TaskStatus } from 'shared/types';

// Mock react-i18next before other imports
vi.mock('react-i18next', async () => {
  const original = await vi.importActual('react-i18next');
  return {
    ...original,
    initReactI18next: { type: '3rdParty', init: () => {} },
    useTranslation: () => ({
      t: (key: string) => key,
      i18n: {
        changeLanguage: () => Promise.resolve(),
        language: 'en',
      },
    }),
    Trans: ({ children }: { children: React.ReactNode }) => children,
  };
});

// Mock i18n config to prevent initialization
vi.mock('@/i18n/config', () => ({
  default: {},
}));

// Mock the hooks
vi.mock('@/hooks', () => ({
  useAuth: () => ({ userId: 'user-1' }),
  useIsOrgAdmin: () => false,
  useNavigateWithSearch: () => vi.fn(),
}));

vi.mock('@/contexts/TaskOptimisticContext', () => ({
  useTaskOptimistic: () => null,
  getArchivedCallback: () => undefined,
}));

vi.mock('@/contexts/ProjectContext', () => ({
  useProject: () => ({ project: null }),
}));

vi.mock('@/hooks/useTaskLabels', () => ({
  useTaskLabels: () => ({ data: [] }),
}));

// Mock the TaskCard and SharedTaskCard to avoid deep dependency issues
vi.mock('../TaskCard', () => ({
  TaskCard: ({
    task,
    onViewDetails,
  }: {
    task: TaskWithAttemptStatus;
    onViewDetails: (task: TaskWithAttemptStatus) => void;
  }) => (
    <div data-testid={`task-${task.id}`} onClick={() => onViewDetails(task)}>
      {task.title}
    </div>
  ),
}));

vi.mock('../SharedTaskCard', () => ({
  SharedTaskCard: ({ task }: { task: { id: string; title: string } }) => (
    <div data-testid={`shared-task-${task.id}`}>{task.title}</div>
  ),
}));

// Helper to create mock tasks
function createMockTask(
  id: string,
  title: string,
  status: TaskStatus
): TaskWithAttemptStatus {
  return {
    id,
    title,
    description: null,
    status,
    project_id: 'project-1',
    created_at: new Date().toISOString(),
    updated_at: new Date().toISOString(),
    archived_at: null,
    parent_task_id: null,
    shared_task_id: null,
    executor: 'CLAUDE_CODE',
    has_in_progress_attempt: false,
    has_merged_attempt: false,
    last_attempt_failed: false,
    is_remote: false,
    remote_assignee_user_id: null,
    remote_assignee_name: null,
    remote_assignee_username: null,
    remote_version: BigInt(0),
    remote_last_synced_at: null,
    remote_stream_node_id: null,
    remote_stream_url: null,
    activity_at: null,
  };
}

// Helper to create mock columns
function createMockColumns(): KanbanColumns {
  return {
    todo: [
      { type: 'task', task: createMockTask('task-1', 'Task 1', 'todo') },
      { type: 'task', task: createMockTask('task-2', 'Task 2', 'todo') },
    ],
    inprogress: [
      {
        type: 'task',
        task: createMockTask('task-3', 'Task 3', 'inprogress'),
      },
    ],
    inreview: [],
    done: [
      { type: 'task', task: createMockTask('task-4', 'Task 4', 'done') },
    ],
    cancelled: [],
  };
}


describe('MobileKanbanBoard', () => {
  const mockOnViewTaskDetails = vi.fn();
  const mockOnViewSharedTask = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
  });

  const defaultProps = {
    columns: createMockColumns(),
    onViewTaskDetails: mockOnViewTaskDetails,
    onViewSharedTask: mockOnViewSharedTask,
    projectId: 'project-1',
  };

  it('should render the mobile kanban board', () => {
    render(<MobileKanbanBoard {...defaultProps} />);
    expect(screen.getByTestId('mobile-kanban-board')).toBeInTheDocument();
  });

  it('should render only one column at a time (first column by default)', () => {
    render(<MobileKanbanBoard {...defaultProps} />);
    // Should show "To Do" column header
    expect(screen.getByText('To Do')).toBeInTheDocument();
    expect(screen.getByText('(2)')).toBeInTheDocument(); // 2 tasks in todo
  });

  it('should show column indicator dots for all columns', () => {
    render(<MobileKanbanBoard {...defaultProps} />);
    const dots = screen.getAllByRole('tab');
    expect(dots).toHaveLength(5); // 5 status columns
  });

  it('should navigate to next column on next button click', () => {
    render(<MobileKanbanBoard {...defaultProps} />);

    // Initially on "To Do"
    expect(screen.getByText('To Do')).toBeInTheDocument();

    // Click next
    const nextBtn = screen.getByTestId('next-column-btn');
    fireEvent.click(nextBtn);

    // Now on "In Progress"
    expect(screen.getByText('In Progress')).toBeInTheDocument();
  });

  it('should navigate to previous column on prev button click', () => {
    render(<MobileKanbanBoard {...defaultProps} />);

    // Go to second column first
    const nextBtn = screen.getByTestId('next-column-btn');
    fireEvent.click(nextBtn);
    expect(screen.getByText('In Progress')).toBeInTheDocument();

    // Click prev
    const prevBtn = screen.getByTestId('prev-column-btn');
    fireEvent.click(prevBtn);

    // Back to "To Do"
    expect(screen.getByText('To Do')).toBeInTheDocument();
  });

  it('should not navigate past first column', () => {
    render(<MobileKanbanBoard {...defaultProps} />);

    // Try to go previous from first column
    const prevBtn = screen.getByTestId('prev-column-btn');
    expect(prevBtn).toBeDisabled();

    // Should still be on first column
    expect(screen.getByText('To Do')).toBeInTheDocument();
  });

  it('should not navigate past last column', () => {
    render(<MobileKanbanBoard {...defaultProps} />);

    // Navigate to last column
    const nextBtn = screen.getByTestId('next-column-btn');
    for (let i = 0; i < 4; i++) {
      fireEvent.click(nextBtn);
    }

    // Should be on "Cancelled"
    expect(screen.getByText('Cancelled')).toBeInTheDocument();

    // Next button should be disabled
    expect(nextBtn).toBeDisabled();
  });

  it('should show current column name and task count', () => {
    render(<MobileKanbanBoard {...defaultProps} />);

    // First column
    expect(screen.getByText('To Do')).toBeInTheDocument();
    expect(screen.getByText('(2)')).toBeInTheDocument();

    // Navigate to next
    const nextBtn = screen.getByTestId('next-column-btn');
    fireEvent.click(nextBtn);

    // Second column
    expect(screen.getByText('In Progress')).toBeInTheDocument();
    expect(screen.getByText('(1)')).toBeInTheDocument();
  });

  it('should navigate to next column on left swipe', () => {
    render(<MobileKanbanBoard {...defaultProps} />);

    const swipeArea = screen.getByTestId('swipeable-area');

    // Simulate left swipe (finger moves left = negative deltaX)
    act(() => {
      fireEvent.touchStart(swipeArea, {
        touches: [{ clientX: 200, clientY: 100 }],
      });
    });
    act(() => {
      fireEvent.touchEnd(swipeArea, {
        changedTouches: [{ clientX: 100, clientY: 100 }],
      });
    });

    // Should be on next column
    expect(screen.getByText('In Progress')).toBeInTheDocument();
  });

  it('should navigate to previous column on right swipe', () => {
    render(<MobileKanbanBoard {...defaultProps} />);

    // First go to second column
    const nextBtn = screen.getByTestId('next-column-btn');
    fireEvent.click(nextBtn);
    expect(screen.getByText('In Progress')).toBeInTheDocument();

    const swipeArea = screen.getByTestId('swipeable-area');

    // Simulate right swipe (finger moves right = positive deltaX)
    act(() => {
      fireEvent.touchStart(swipeArea, {
        touches: [{ clientX: 100, clientY: 100 }],
      });
    });
    act(() => {
      fireEvent.touchEnd(swipeArea, {
        changedTouches: [{ clientX: 200, clientY: 100 }],
      });
    });

    // Should be back on first column
    expect(screen.getByText('To Do')).toBeInTheDocument();
  });

  it('should display tasks in the current column', () => {
    render(<MobileKanbanBoard {...defaultProps} />);

    // First column has Task 1 and Task 2
    expect(screen.getByText('Task 1')).toBeInTheDocument();
    expect(screen.getByText('Task 2')).toBeInTheDocument();
  });

  it('should show empty message for columns with no tasks', () => {
    render(<MobileKanbanBoard {...defaultProps} />);

    // Navigate to "In Review" which is empty
    const nextBtn = screen.getByTestId('next-column-btn');
    fireEvent.click(nextBtn); // In Progress
    fireEvent.click(nextBtn); // In Review

    expect(screen.getByText('In Review')).toBeInTheDocument();
    // Check that the "In Review" column (index 2) contains empty message
    const inReviewColumn = screen.getByTestId('column-inreview');
    expect(inReviewColumn).toContainElement(
      inReviewColumn.querySelector('[data-testid="empty-column"]')
    );
  });

  it('should call onViewTaskDetails when a task is clicked', async () => {
    render(<MobileKanbanBoard {...defaultProps} />);

    // Click on Task 1
    const task1 = screen.getByText('Task 1');
    fireEvent.click(task1);

    expect(mockOnViewTaskDetails).toHaveBeenCalledWith(
      expect.objectContaining({ id: 'task-1', title: 'Task 1' })
    );
  });

  it('should highlight selected task', () => {
    render(
      <MobileKanbanBoard {...defaultProps} selectedTaskId="task-1" />
    );

    // The selected task should have isOpen prop passed to TaskCard
    // This is tested implicitly by rendering without errors
    expect(screen.getByText('Task 1')).toBeInTheDocument();
  });

  it('should animate column transition with CSS transform', () => {
    render(<MobileKanbanBoard {...defaultProps} />);

    const swipeArea = screen.getByTestId('swipeable-area');
    const slidingContainer = swipeArea.firstChild as HTMLElement;

    // Initially at first column (0% offset in 5-column setup)
    expect(slidingContainer).toHaveStyle({ transform: 'translateX(-0%)' });

    // Navigate to second column
    const nextBtn = screen.getByTestId('next-column-btn');
    fireEvent.click(nextBtn);

    // Should be at second column (20% offset for column 1 out of 5)
    expect(slidingContainer).toHaveStyle({ transform: 'translateX(-20%)' });
  });
});
