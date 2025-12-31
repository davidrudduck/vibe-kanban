import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { TodosBadge } from '../TodosBadge';
import { TaskInfoSheet } from '../TaskInfoSheet';
import type { TodoItem, TaskWithAttemptStatus } from 'shared/types';

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

// Mock framer-motion to avoid animation issues in tests
vi.mock('framer-motion', async () => {
  const actual = await vi.importActual('framer-motion');
  return {
    ...actual,
    AnimatePresence: ({ children }: { children: React.ReactNode }) => children,
    motion: {
      div: ({
        children,
        className,
        onClick,
        ...props
      }: {
        children: React.ReactNode;
        className?: string;
        onClick?: () => void;
        [key: string]: unknown;
      }) => (
        <div className={className} onClick={onClick} {...props}>
          {children}
        </div>
      ),
    },
  };
});

// Helper to create mock todos with all required fields
function createMockTodos(): TodoItem[] {
  return [
    { content: 'First todo', status: 'pending', priority: null },
    { content: 'Second todo', status: 'in_progress', priority: null },
    { content: 'Third todo', status: 'completed', priority: null },
  ];
}

// Helper to create mock task
function createMockTask(
  overrides?: Partial<TaskWithAttemptStatus>
): TaskWithAttemptStatus {
  const now = new Date();
  const nowStr = now.toISOString();
  return {
    id: 'task-1',
    title: 'Test Task',
    description: 'Test description',
    status: 'inprogress',
    project_id: 'project-1',
    parent_task_id: null,
    swarm_task_id: null,
    archived_at: null,
    created_at: nowStr,
    updated_at: nowStr,
    activity_at: now,
    has_in_progress_attempt: false,
    has_merged_attempt: false,
    last_attempt_failed: false,
    executor: 'CLAUDE_CODE',
    is_remote: false,
    remote_assignee_user_id: null,
    remote_assignee_name: null,
    remote_assignee_username: null,
    remote_version: BigInt(0),
    remote_last_synced_at: null,
    remote_stream_node_id: null,
    remote_stream_url: null,
    ...overrides,
  };
}

describe('TodosBadge', () => {
  it('should render nothing when todos is empty', () => {
    const { container } = render(<TodosBadge todos={[]} />);
    expect(container.firstChild).toBeNull();
  });

  it('should render nothing when todos is undefined', () => {
    const { container } = render(
      <TodosBadge todos={undefined as unknown as TodoItem[]} />
    );
    expect(container.firstChild).toBeNull();
  });

  it('should display pending count in badge', () => {
    const todos = createMockTodos();
    render(<TodosBadge todos={todos} />);
    // 2 pending (pending + in_progress), 1 completed
    expect(screen.getByText('2')).toBeInTheDocument();
  });

  it('should show todo count when all completed', () => {
    const todos: TodoItem[] = [
      { content: 'Done 1', status: 'completed', priority: null },
      { content: 'Done 2', status: 'completed', priority: null },
    ];
    render(<TodosBadge todos={todos} />);
    // 0 pending
    expect(screen.getByText('0')).toBeInTheDocument();
  });

  it('should have accessible label', () => {
    const todos = createMockTodos();
    render(<TodosBadge todos={todos} />);
    const button = screen.getByRole('button');
    expect(button).toHaveAttribute('aria-label', '2 todos pending');
  });

  it('should open popover when clicked', () => {
    const todos = createMockTodos();
    render(<TodosBadge todos={todos} />);
    const button = screen.getByRole('button');
    fireEvent.click(button);
    // Popover should show all todos
    expect(screen.getByText('First todo')).toBeInTheDocument();
    expect(screen.getByText('Second todo')).toBeInTheDocument();
    expect(screen.getByText('Third todo')).toBeInTheDocument();
  });
});

describe('TaskInfoSheet', () => {
  const mockOnOpenChange = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('should not render when closed', () => {
    const task = createMockTask();
    const { container } = render(
      <TaskInfoSheet
        task={task}
        isOpen={false}
        onOpenChange={mockOnOpenChange}
      />
    );
    // AnimatePresence should not render children when not open
    expect(
      container.querySelector('[aria-hidden="true"]')
    ).not.toBeInTheDocument();
  });

  it('should render when open', () => {
    const task = createMockTask();
    render(
      <TaskInfoSheet
        task={task}
        isOpen={true}
        onOpenChange={mockOnOpenChange}
      />
    );
    expect(screen.getByText('Task Info')).toBeInTheDocument();
  });

  it('should display task description', () => {
    const task = createMockTask({ description: 'This is a test description' });
    render(
      <TaskInfoSheet
        task={task}
        isOpen={true}
        onOpenChange={mockOnOpenChange}
      />
    );
    expect(screen.getByText('Description')).toBeInTheDocument();
  });

  it('should display relationships when provided', () => {
    const task = createMockTask();
    render(
      <TaskInfoSheet
        task={task}
        isOpen={true}
        onOpenChange={mockOnOpenChange}
        relationships={<div>Parent Task Link</div>}
      />
    );
    expect(screen.getByText('Relationships')).toBeInTheDocument();
    expect(screen.getByText('Parent Task Link')).toBeInTheDocument();
  });

  it('should display variables when provided', () => {
    const task = createMockTask();
    render(
      <TaskInfoSheet
        task={task}
        isOpen={true}
        onOpenChange={mockOnOpenChange}
        variables={<div>$API_KEY</div>}
      />
    );
    expect(screen.getByText('Variables')).toBeInTheDocument();
    expect(screen.getByText('$API_KEY')).toBeInTheDocument();
  });

  it('should call onOpenChange when close button clicked', () => {
    const task = createMockTask();
    render(
      <TaskInfoSheet
        task={task}
        isOpen={true}
        onOpenChange={mockOnOpenChange}
      />
    );
    const closeButton = screen.getByRole('button', { name: /close/i });
    fireEvent.click(closeButton);
    expect(mockOnOpenChange).toHaveBeenCalledWith(false);
  });

  it('should call onOpenChange when backdrop clicked', () => {
    const task = createMockTask();
    render(
      <TaskInfoSheet
        task={task}
        isOpen={true}
        onOpenChange={mockOnOpenChange}
      />
    );
    const backdrop = document.querySelector('[aria-hidden="true"]');
    expect(backdrop).toBeInTheDocument();
    if (backdrop) {
      fireEvent.click(backdrop);
      expect(mockOnOpenChange).toHaveBeenCalledWith(false);
    }
  });
});
