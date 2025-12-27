import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent, act } from '@testing-library/react';
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

// Mock i18n config
vi.mock('@/i18n/config', () => ({
  default: {},
}));

// Mock hooks
vi.mock('@/hooks', () => ({
  useAuth: () => ({ userId: 'user-1' }),
  useIsOrgAdmin: () => false,
  useNavigateWithSearch: () => vi.fn(),
}));

// Mock useIsMobile hook
const mockUseIsMobile = vi.fn(() => true);
vi.mock('@/hooks/useIsMobile', () => ({
  useIsMobile: () => mockUseIsMobile(),
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

// Mock tasksApi
const mockArchive = vi.fn().mockResolvedValue({});
vi.mock('@/lib/api', () => ({
  tasksApi: {
    archive: () => mockArchive(),
  },
}));

// Import after mocks
import { SwipeableTaskCard } from '../SwipeableTaskCard';

// Helper to create mock tasks
function createMockTask(
  id: string,
  title: string,
  status: TaskStatus,
  archivedAt: Date | null = null
): TaskWithAttemptStatus {
  return {
    id,
    title,
    description: null,
    status,
    project_id: 'project-1',
    created_at: new Date().toISOString(),
    updated_at: new Date().toISOString(),
    archived_at: archivedAt,
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

describe('SwipeableTaskCard - swipe-to-archive', () => {
  const mockOnArchive = vi.fn();
  const mockTask = createMockTask('task-1', 'Test Task', 'todo');

  beforeEach(() => {
    vi.clearAllMocks();
    mockUseIsMobile.mockReturnValue(true);
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  const defaultProps = {
    task: mockTask,
    onArchive: mockOnArchive,
    isArchived: false,
    disabled: false,
    children: <div data-testid="task-content">Task Content</div>,
  };

  it('should render children content', () => {
    render(<SwipeableTaskCard {...defaultProps} />);
    expect(screen.getByTestId('task-content')).toBeInTheDocument();
  });

  it('should show archive indicator when swiping left', async () => {
    render(<SwipeableTaskCard {...defaultProps} />);
    const swipeableElement = screen.getByTestId('swipeable-task-card');

    // Start swipe
    act(() => {
      fireEvent.touchStart(swipeableElement, {
        touches: [{ clientX: 300, clientY: 100 }],
      });
    });

    // Move left (shows archive indicator)
    act(() => {
      fireEvent.touchMove(swipeableElement, {
        touches: [{ clientX: 200, clientY: 100 }],
      });
    });

    // Archive indicator should be visible
    expect(screen.getByTestId('archive-indicator')).toBeInTheDocument();
  });

  it('should archive task when swipe exceeds threshold (100px)', () => {
    render(<SwipeableTaskCard {...defaultProps} />);
    const swipeableElement = screen.getByTestId('swipeable-task-card');

    // Swipe left past threshold
    act(() => {
      fireEvent.touchStart(swipeableElement, {
        touches: [{ clientX: 300, clientY: 100 }],
      });
    });

    // Move during swipe to set the offset
    act(() => {
      fireEvent.touchMove(swipeableElement, {
        touches: [{ clientX: 150, clientY: 100 }],
      });
    });

    act(() => {
      fireEvent.touchEnd(swipeableElement, {
        changedTouches: [{ clientX: 150, clientY: 100 }],
      });
    });

    // Should call onArchive synchronously
    expect(mockOnArchive).toHaveBeenCalledWith(mockTask);
  });

  it('should snap back when swipe cancelled (below threshold)', async () => {
    render(<SwipeableTaskCard {...defaultProps} />);
    const swipeableElement = screen.getByTestId('swipeable-task-card');
    const slidingContent = screen.getByTestId('sliding-content');

    // Start swipe
    act(() => {
      fireEvent.touchStart(swipeableElement, {
        touches: [{ clientX: 300, clientY: 100 }],
      });
    });

    // Move left but not past threshold
    act(() => {
      fireEvent.touchMove(swipeableElement, {
        touches: [{ clientX: 250, clientY: 100 }],
      });
    });

    // End swipe
    act(() => {
      fireEvent.touchEnd(swipeableElement, {
        changedTouches: [{ clientX: 250, clientY: 100 }],
      });
    });

    // Should not call onArchive
    expect(mockOnArchive).not.toHaveBeenCalled();

    // Sliding content should snap back to original position
    act(() => {
      vi.advanceTimersByTime(300); // Wait for animation
    });

    expect(slidingContent).toHaveStyle({ transform: 'translateX(0px)' });
  });

  it('should show confirmation visual during swipe', () => {
    render(<SwipeableTaskCard {...defaultProps} />);
    const swipeableElement = screen.getByTestId('swipeable-task-card');

    // Swipe left
    act(() => {
      fireEvent.touchStart(swipeableElement, {
        touches: [{ clientX: 300, clientY: 100 }],
      });
    });

    act(() => {
      fireEvent.touchMove(swipeableElement, {
        touches: [{ clientX: 150, clientY: 100 }],
      });
    });

    // Should show red background indicating archive action
    const archiveIndicator = screen.getByTestId('archive-indicator');
    expect(archiveIndicator).toHaveClass('bg-destructive');
  });

  it('should not activate swipe when swiping right (reserved for other actions)', () => {
    render(<SwipeableTaskCard {...defaultProps} />);
    const swipeableElement = screen.getByTestId('swipeable-task-card');

    // Swipe right (positive deltaX)
    act(() => {
      fireEvent.touchStart(swipeableElement, {
        touches: [{ clientX: 100, clientY: 100 }],
      });
    });

    act(() => {
      fireEvent.touchMove(swipeableElement, {
        touches: [{ clientX: 250, clientY: 100 }],
      });
    });

    act(() => {
      fireEvent.touchEnd(swipeableElement, {
        changedTouches: [{ clientX: 250, clientY: 100 }],
      });
    });

    // Archive indicator exists but should have opacity-0 (hidden)
    const archiveIndicator = screen.getByTestId('archive-indicator');
    expect(archiveIndicator).toHaveClass('opacity-0');
    expect(mockOnArchive).not.toHaveBeenCalled();
  });

  it('should disable swipe on already archived tasks', () => {
    const archivedTask = createMockTask(
      'task-2',
      'Archived Task',
      'todo',
      new Date()
    );

    render(
      <SwipeableTaskCard
        {...defaultProps}
        task={archivedTask}
        isArchived={true}
      />
    );

    const swipeableElement = screen.getByTestId('swipeable-task-card');

    // Try to swipe
    act(() => {
      fireEvent.touchStart(swipeableElement, {
        touches: [{ clientX: 300, clientY: 100 }],
      });
    });

    act(() => {
      fireEvent.touchMove(swipeableElement, {
        touches: [{ clientX: 100, clientY: 100 }],
      });
    });

    // Archive indicator should not appear
    expect(screen.queryByTestId('archive-indicator')).not.toBeInTheDocument();
  });

  it('should disable swipe when disabled prop is true', () => {
    render(<SwipeableTaskCard {...defaultProps} disabled={true} />);

    const swipeableElement = screen.getByTestId('swipeable-task-card');

    // Try to swipe
    act(() => {
      fireEvent.touchStart(swipeableElement, {
        touches: [{ clientX: 300, clientY: 100 }],
      });
    });

    act(() => {
      fireEvent.touchMove(swipeableElement, {
        touches: [{ clientX: 100, clientY: 100 }],
      });
    });

    // Archive indicator should not appear
    expect(screen.queryByTestId('archive-indicator')).not.toBeInTheDocument();
  });

  it('should not show swipe on desktop (non-mobile)', () => {
    mockUseIsMobile.mockReturnValue(false);

    render(<SwipeableTaskCard {...defaultProps} />);

    // Should not have swipeable functionality on desktop
    expect(
      screen.queryByTestId('swipeable-task-card')
    ).not.toBeInTheDocument();

    // Content should still render
    expect(screen.getByTestId('task-content')).toBeInTheDocument();
  });

  it('should show archive icon in the indicator when threshold is exceeded', () => {
    render(<SwipeableTaskCard {...defaultProps} />);
    const swipeableElement = screen.getByTestId('swipeable-task-card');

    // Swipe past threshold
    act(() => {
      fireEvent.touchStart(swipeableElement, {
        touches: [{ clientX: 300, clientY: 100 }],
      });
    });

    act(() => {
      fireEvent.touchMove(swipeableElement, {
        touches: [{ clientX: 150, clientY: 100 }],
      });
    });

    // Archive icon should be visible
    expect(screen.getByTestId('archive-icon')).toBeInTheDocument();
  });
});
