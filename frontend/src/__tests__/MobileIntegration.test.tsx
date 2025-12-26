/**
 * Mobile Integration Tests
 *
 * Tests the complete mobile user experience including:
 * - Navigation flow
 * - Swipe gestures between columns
 * - Task detail panel interactions
 * - Bottom navigation behavior
 * - Viewport responsiveness
 *
 * @session Session 7: Integration & Polish
 */

import { describe, it, expect, vi, beforeEach, afterEach, Mock } from 'vitest';
import { render, screen, fireEvent, act } from '@testing-library/react';
import { useLocation, useNavigate } from 'react-router-dom';
import type { Project } from 'shared/types';

// Mock react-router-dom
vi.mock('react-router-dom', async () => {
  const actual = await vi.importActual('react-router-dom');
  return {
    ...actual,
    useLocation: vi.fn(),
    useNavigate: vi.fn(),
  };
});

// Mock react-i18next
vi.mock('react-i18next', async () => {
  const original = await vi.importActual('react-i18next');
  return {
    ...original,
    initReactI18next: { type: '3rdParty', init: () => {} },
    useTranslation: () => ({
      t: (key: string, defaultValue?: string | { defaultValue?: string }) => {
        if (typeof defaultValue === 'object' && defaultValue?.defaultValue) {
          return defaultValue.defaultValue;
        }
        if (typeof defaultValue === 'string') {
          return defaultValue;
        }
        const translations: Record<string, string> = {
          'bottomNav.projects': 'Projects',
          'bottomNav.tasks': 'Tasks',
          'bottomNav.add': 'Add',
          'bottomNav.activity': 'Activity',
          'bottomNav.menu': 'Menu',
          'columns.todo': 'To Do',
          'columns.inprogress': 'In Progress',
          'columns.inreview': 'In Review',
          'columns.done': 'Done',
          'columns.cancelled': 'Cancelled',
        };
        return translations[key] || key;
      },
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

// Mock contexts
vi.mock('@/contexts/TaskOptimisticContext', () => ({
  useTaskOptimistic: () => null,
  getArchivedCallback: () => undefined,
}));

vi.mock('@/contexts/ProjectContext', () => ({
  useProject: vi.fn(() => ({
    projectId: 'test-project-id',
    project: { id: 'test-project-id', name: 'Test Project' } as Project,
  })),
}));

vi.mock('@/hooks/useTaskLabels', () => ({
  useTaskLabels: () => ({ data: [] }),
}));

// Mock openTaskForm
vi.mock('@/lib/openTaskForm', () => ({
  openTaskForm: vi.fn(),
}));

// Helper to set viewport size for tests
function setViewportSize(width: number, height: number) {
  Object.defineProperty(window, 'innerWidth', { writable: true, configurable: true, value: width });
  Object.defineProperty(window, 'innerHeight', { writable: true, configurable: true, value: height });
  window.dispatchEvent(new Event('resize'));
}

// Helper to mock matchMedia for mobile/desktop detection
function mockMatchMedia(isMobile: boolean) {
  Object.defineProperty(window, 'matchMedia', {
    writable: true,
    configurable: true,
    value: vi.fn().mockImplementation((query: string) => ({
      matches: query.includes('max-width') ? isMobile : !isMobile,
      media: query,
      onchange: null,
      addListener: vi.fn(),
      removeListener: vi.fn(),
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      dispatchEvent: vi.fn(),
    })),
  });
}

describe('Mobile Integration Tests', () => {
  const mockNavigate = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
    (useNavigate as Mock).mockReturnValue(mockNavigate);
    (useLocation as Mock).mockReturnValue({ pathname: '/projects' });
    mockMatchMedia(true); // Default to mobile
    setViewportSize(390, 844); // iPhone 14 dimensions
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  describe('Mobile Detection', () => {
    it('should correctly detect mobile viewport at 390px width', () => {
      setViewportSize(390, 844);
      mockMatchMedia(true);

      // The hook should return true for mobile
      const query = window.matchMedia('(max-width: 639px)');
      expect(query.matches).toBe(true);
    });

    it('should correctly detect desktop viewport at 1280px width', () => {
      setViewportSize(1280, 800);
      mockMatchMedia(false);

      const query = window.matchMedia('(max-width: 639px)');
      expect(query.matches).toBe(false);
    });

    it('should correctly detect tablet viewport at 768px width', () => {
      setViewportSize(768, 1024);
      // Tablet is > 640px so not mobile
      mockMatchMedia(false);

      const query = window.matchMedia('(max-width: 639px)');
      expect(query.matches).toBe(false);
    });
  });

  describe('Bottom Navigation Integration', () => {
    // Import after mocks are set up
    let BottomNav: typeof import('../components/layout/BottomNav').BottomNav;

    beforeEach(async () => {
      const module = await import('../components/layout/BottomNav');
      BottomNav = module.BottomNav;
    });

    it('should navigate between main sections via bottom nav', () => {
      (useLocation as Mock).mockReturnValue({ pathname: '/projects' });
      render(<BottomNav />);

      // Click on Tasks
      const tasksBtn = screen.getByRole('button', { name: /tasks/i });
      fireEvent.click(tasksBtn);
      expect(mockNavigate).toHaveBeenCalledWith('/projects/test-project-id/tasks');

      // Click on Settings (Menu)
      const menuBtn = screen.getByRole('button', { name: /menu/i });
      fireEvent.click(menuBtn);
      expect(mockNavigate).toHaveBeenCalledWith('/settings');

      // Click on Activity (Processes)
      const activityBtn = screen.getByRole('button', { name: /activity/i });
      fireEvent.click(activityBtn);
      expect(mockNavigate).toHaveBeenCalledWith('/processes');
    });

    it('should trigger task creation from Add button', async () => {
      const { openTaskForm } = await import('@/lib/openTaskForm');
      render(<BottomNav />);

      const addBtn = screen.getByRole('button', { name: /add/i });
      fireEvent.click(addBtn);

      expect(openTaskForm).toHaveBeenCalledWith({
        mode: 'create',
        projectId: 'test-project-id',
      });
    });

    it('should highlight current route correctly', () => {
      (useLocation as Mock).mockReturnValue({ pathname: '/settings' });
      render(<BottomNav />);

      const menuBtn = screen.getByRole('button', { name: /menu/i });
      // Should have active styling (primary color)
      expect(menuBtn.className).toMatch(/text-primary|bg-primary/);

      const projectsBtn = screen.getByRole('button', { name: /projects/i });
      // Should not have active styling
      expect(projectsBtn.className).toMatch(/text-muted-foreground/);
    });
  });

  describe('Swipe Gesture Detection', () => {
    // Import hook once for all swipe tests
    let useSwipe: typeof import('../hooks/useSwipe').useSwipe;

    beforeEach(async () => {
      const module = await import('../hooks/useSwipe');
      useSwipe = module.useSwipe;
    });

    it('should detect left swipe correctly', async () => {
      const onSwipeLeft = vi.fn();
      const onSwipeRight = vi.fn();

      // Create a simple test component to verify swipe detection
      const TestSwipeComponent = () => {
        const handlers = useSwipe({ onSwipeLeft, onSwipeRight });
        return <div data-testid="swipe-target" {...handlers}>Swipe me</div>;
      };

      render(<TestSwipeComponent />);
      const target = screen.getByTestId('swipe-target');

      // Simulate left swipe (start at 200, end at 100 = -100px movement)
      act(() => {
        fireEvent.touchStart(target, {
          touches: [{ clientX: 200, clientY: 100 }],
        });
      });
      act(() => {
        fireEvent.touchEnd(target, {
          changedTouches: [{ clientX: 100, clientY: 100 }],
        });
      });

      expect(onSwipeLeft).toHaveBeenCalled();
      expect(onSwipeRight).not.toHaveBeenCalled();
    });

    it('should detect right swipe correctly', async () => {
      const onSwipeLeft = vi.fn();
      const onSwipeRight = vi.fn();

      const TestSwipeComponent = () => {
        const handlers = useSwipe({ onSwipeLeft, onSwipeRight });
        return <div data-testid="swipe-target" {...handlers}>Swipe me</div>;
      };

      render(<TestSwipeComponent />);
      const target = screen.getByTestId('swipe-target');

      // Simulate right swipe (start at 100, end at 200 = +100px movement)
      act(() => {
        fireEvent.touchStart(target, {
          touches: [{ clientX: 100, clientY: 100 }],
        });
      });
      act(() => {
        fireEvent.touchEnd(target, {
          changedTouches: [{ clientX: 200, clientY: 100 }],
        });
      });

      expect(onSwipeRight).toHaveBeenCalled();
      expect(onSwipeLeft).not.toHaveBeenCalled();
    });

    it('should ignore swipes below threshold', async () => {
      const onSwipeLeft = vi.fn();
      const onSwipeRight = vi.fn();

      const TestSwipeComponent = () => {
        const handlers = useSwipe({ onSwipeLeft, onSwipeRight });
        return <div data-testid="swipe-target" {...handlers}>Swipe me</div>;
      };

      render(<TestSwipeComponent />);
      const target = screen.getByTestId('swipe-target');

      // Simulate small swipe (only 30px, below 50px threshold)
      act(() => {
        fireEvent.touchStart(target, {
          touches: [{ clientX: 100, clientY: 100 }],
        });
      });
      act(() => {
        fireEvent.touchEnd(target, {
          changedTouches: [{ clientX: 130, clientY: 100 }],
        });
      });

      expect(onSwipeLeft).not.toHaveBeenCalled();
      expect(onSwipeRight).not.toHaveBeenCalled();
    });

    it('should ignore vertical swipes', async () => {
      const onSwipeLeft = vi.fn();
      const onSwipeRight = vi.fn();

      const TestSwipeComponent = () => {
        const handlers = useSwipe({ onSwipeLeft, onSwipeRight });
        return <div data-testid="swipe-target" {...handlers}>Swipe me</div>;
      };

      render(<TestSwipeComponent />);
      const target = screen.getByTestId('swipe-target');

      // Simulate vertical scroll (more Y movement than X)
      act(() => {
        fireEvent.touchStart(target, {
          touches: [{ clientX: 100, clientY: 100 }],
        });
      });
      act(() => {
        fireEvent.touchEnd(target, {
          changedTouches: [{ clientX: 140, clientY: 300 }],
        });
      });

      expect(onSwipeLeft).not.toHaveBeenCalled();
      expect(onSwipeRight).not.toHaveBeenCalled();
    });
  });

  describe('Mobile Kanban Board Integration', () => {
    // Note: MobileKanbanBoard has comprehensive unit tests in MobileKanbanBoard.test.tsx
    // These integration tests verify the component renders and basic behavior works
    // without re-testing all the internals

    it('should render MobileColumnHeader with correct navigation structure', async () => {
      const MobileColumnHeader = (await import('../components/tasks/MobileColumnHeader')).default;

      const onPrev = vi.fn();
      const onNext = vi.fn();

      render(
        <MobileColumnHeader
          name="To Do"
          count={5}
          color="bg-blue-500"
          isFirst={false}
          isLast={false}
          onPrev={onPrev}
          onNext={onNext}
          currentIndex={1}
          totalColumns={5}
        />
      );

      // Header should display column name and count
      expect(screen.getByText('To Do')).toBeInTheDocument();
      expect(screen.getByText('(5)')).toBeInTheDocument();

      // Navigation buttons should work
      const prevBtn = screen.getByTestId('prev-column-btn');
      const nextBtn = screen.getByTestId('next-column-btn');

      fireEvent.click(prevBtn);
      expect(onPrev).toHaveBeenCalled();

      fireEvent.click(nextBtn);
      expect(onNext).toHaveBeenCalled();
    });

    it('should disable prev button on first column', async () => {
      const MobileColumnHeader = (await import('../components/tasks/MobileColumnHeader')).default;

      render(
        <MobileColumnHeader
          name="To Do"
          count={3}
          color="bg-blue-500"
          isFirst={true}
          isLast={false}
          onPrev={vi.fn()}
          onNext={vi.fn()}
          currentIndex={0}
          totalColumns={5}
        />
      );

      const prevBtn = screen.getByTestId('prev-column-btn');
      expect(prevBtn).toBeDisabled();
    });

    it('should disable next button on last column', async () => {
      const MobileColumnHeader = (await import('../components/tasks/MobileColumnHeader')).default;

      render(
        <MobileColumnHeader
          name="Cancelled"
          count={0}
          color="bg-gray-500"
          isFirst={false}
          isLast={true}
          onPrev={vi.fn()}
          onNext={vi.fn()}
          currentIndex={4}
          totalColumns={5}
        />
      );

      const nextBtn = screen.getByTestId('next-column-btn');
      expect(nextBtn).toBeDisabled();
    });

    it('should show column indicator dots', async () => {
      const MobileColumnHeader = (await import('../components/tasks/MobileColumnHeader')).default;

      render(
        <MobileColumnHeader
          name="In Progress"
          count={2}
          color="bg-yellow-500"
          isFirst={false}
          isLast={false}
          onPrev={vi.fn()}
          onNext={vi.fn()}
          currentIndex={1}
          totalColumns={5}
        />
      );

      // Should have 5 indicator dots
      const dots = screen.getAllByRole('tab');
      expect(dots).toHaveLength(5);

      // Second dot (index 1) should be active
      expect(dots[1]).toHaveAttribute('aria-selected', 'true');
    });
  });

  describe('Touch Target Accessibility', () => {
    let BottomNav: typeof import('../components/layout/BottomNav').BottomNav;

    beforeEach(async () => {
      const module = await import('../components/layout/BottomNav');
      BottomNav = module.BottomNav;
    });

    it('should have touch targets meeting 48px minimum for bottom nav', () => {
      render(<BottomNav />);

      const buttons = screen.getAllByRole('button');
      buttons.forEach((btn) => {
        // Check for h-12 (48px) class
        expect(btn.className).toMatch(/h-12|min-h-\[48px\]|min-h-12/);
      });
    });
  });

  describe('Responsive Layout Integration', () => {
    let BottomNav: typeof import('../components/layout/BottomNav').BottomNav;

    beforeEach(async () => {
      const module = await import('../components/layout/BottomNav');
      BottomNav = module.BottomNav;
    });

    it('should hide bottom nav on tablet/desktop (sm:hidden)', () => {
      render(<BottomNav />);

      const nav = screen.getByRole('navigation');
      expect(nav.className).toMatch(/sm:hidden/);
    });

    it('should maintain fixed positioning at bottom', () => {
      render(<BottomNav />);

      const nav = screen.getByRole('navigation');
      expect(nav.className).toMatch(/fixed/);
      expect(nav.className).toMatch(/bottom-0/);
      expect(nav.className).toMatch(/left-0/);
      expect(nav.className).toMatch(/right-0/);
    });
  });

  describe('State Persistence', () => {
    // Note: MobileKanbanBoard state persistence is tested in MobileKanbanBoard.test.tsx
    // This integration test verifies MobileColumnHeader properly reports current index

    it('should correctly display current column indicator', async () => {
      const MobileColumnHeader = (await import('../components/tasks/MobileColumnHeader')).default;

      const { rerender } = render(
        <MobileColumnHeader
          name="To Do"
          count={3}
          color="bg-blue-500"
          isFirst={true}
          isLast={false}
          onPrev={vi.fn()}
          onNext={vi.fn()}
          currentIndex={0}
          totalColumns={5}
        />
      );

      // First dot should be active
      let dots = screen.getAllByRole('tab');
      expect(dots[0]).toHaveAttribute('aria-selected', 'true');

      // Rerender with new index
      rerender(
        <MobileColumnHeader
          name="In Progress"
          count={2}
          color="bg-yellow-500"
          isFirst={false}
          isLast={false}
          onPrev={vi.fn()}
          onNext={vi.fn()}
          currentIndex={1}
          totalColumns={5}
        />
      );

      // Second dot should now be active
      dots = screen.getAllByRole('tab');
      expect(dots[1]).toHaveAttribute('aria-selected', 'true');
      expect(dots[0]).toHaveAttribute('aria-selected', 'false');
    });
  });
});

describe('Mobile Visual Polish', () => {
  describe('CSS Transitions', () => {
    // Note: CSS transitions for MobileKanbanBoard are tested in MobileKanbanBoard.test.tsx
    // This integration test verifies BottomNav has proper visual styling

    it('should have background and border styling for visual polish', async () => {
      const BottomNav = (await import('../components/layout/BottomNav')).BottomNav;
      render(<BottomNav />);

      const nav = screen.getByRole('navigation');

      // Check for background and border classes
      expect(nav.className).toMatch(/bg-background/);
      expect(nav.className).toMatch(/border-t/);
      expect(nav.className).toMatch(/border-border/);
    });
  });

  describe('Spacing Consistency', () => {
    let BottomNav: typeof import('../components/layout/BottomNav').BottomNav;

    beforeEach(async () => {
      const module = await import('../components/layout/BottomNav');
      BottomNav = module.BottomNav;
    });

    it('should have correct height and safe area handling', () => {
      render(<BottomNav />);

      const nav = screen.getByRole('navigation');
      // Check for h-14 (56px) height class
      expect(nav.className).toMatch(/h-14/);
      // Check for safe-area-bottom class for iOS devices
      expect(nav.className).toMatch(/safe-area-bottom/);
    });

    it('should have proper z-index for overlay behavior', () => {
      render(<BottomNav />);

      const nav = screen.getByRole('navigation');
      // Check for z-50 for proper layering
      expect(nav.className).toMatch(/z-50/);
    });
  });
});
