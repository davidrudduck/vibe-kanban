import { describe, it, expect, vi, beforeEach, Mock } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { BottomNav } from '../BottomNav';
import { useLocation, useNavigate } from 'react-router-dom';

// Mock react-router-dom
vi.mock('react-router-dom', () => ({
  useLocation: vi.fn(),
  useNavigate: vi.fn(),
}));

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

// Mock ProjectContext
vi.mock('@/contexts/ProjectContext', () => ({
  useProject: vi.fn(() => ({
    projectId: 'test-project-id',
    project: { id: 'test-project-id', name: 'Test Project' },
  })),
}));

// Mock openTaskForm
vi.mock('@/lib/openTaskForm', () => ({
  openTaskForm: vi.fn(),
}));

describe('BottomNav', () => {
  const mockNavigate = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
    (useNavigate as Mock).mockReturnValue(mockNavigate);
    (useLocation as Mock).mockReturnValue({ pathname: '/projects' });
  });

  it('should render 5 navigation items', () => {
    render(<BottomNav />);
    const buttons = screen.getAllByRole('button');
    expect(buttons.length).toBe(5);
  });

  it('should highlight active route for Projects', () => {
    (useLocation as Mock).mockReturnValue({ pathname: '/projects' });
    render(<BottomNav />);
    const projectsBtn = screen.getByRole('button', { name: /projects/i });
    expect(projectsBtn.className).toMatch(/text-primary|bg-primary/);
  });

  it('should highlight active route for Tasks', () => {
    (useLocation as Mock).mockReturnValue({
      pathname: '/projects/123/tasks',
    });
    render(<BottomNav />);
    const tasksBtn = screen.getByRole('button', { name: /tasks/i });
    expect(tasksBtn.className).toMatch(/text-primary|bg-primary/);
  });

  it('should navigate on tap', () => {
    render(<BottomNav />);
    const projectsBtn = screen.getByRole('button', { name: /projects/i });
    fireEvent.click(projectsBtn);
    expect(mockNavigate).toHaveBeenCalledWith('/projects');
  });

  it('should have minimum 48px touch targets', () => {
    render(<BottomNav />);
    const buttons = screen.getAllByRole('button');
    buttons.forEach((btn) => {
      // Check button has appropriate minimum size (h-12 = 48px)
      expect(btn.className).toMatch(/h-12|min-h-\[48px\]|min-h-12/);
    });
  });

  it('should be hidden on desktop (>= sm breakpoint)', () => {
    render(<BottomNav />);
    const nav = screen.getByRole('navigation');
    // Check for sm:hidden class
    expect(nav.className).toMatch(/sm:hidden/);
  });

  it('should be fixed to bottom of viewport', () => {
    render(<BottomNav />);
    const nav = screen.getByRole('navigation');
    expect(nav.className).toMatch(/fixed/);
    expect(nav.className).toMatch(/bottom-0/);
  });

  it('should render with correct height (56px safe area)', () => {
    render(<BottomNav />);
    const nav = screen.getByRole('navigation');
    // Check for h-14 (56px) class
    expect(nav.className).toMatch(/h-14/);
  });

  it('should open task creation on + tap', async () => {
    const { openTaskForm } = await import('@/lib/openTaskForm');
    render(<BottomNav />);
    const addBtn = screen.getByRole('button', { name: /add/i });
    fireEvent.click(addBtn);
    expect(openTaskForm).toHaveBeenCalledWith({
      mode: 'create',
      projectId: 'test-project-id',
    });
  });

  it('should navigate to /tasks/all when Tasks button clicked without projectId', async () => {
    const { useProject } = await import('@/contexts/ProjectContext');
    (useProject as Mock).mockReturnValue({
      projectId: null,
      project: null,
    });
    render(<BottomNav />);
    const tasksBtn = screen.getByRole('button', { name: /tasks/i });
    fireEvent.click(tasksBtn);
    expect(mockNavigate).toHaveBeenCalledWith('/tasks/all');
  });

  it('should navigate to project tasks when Tasks button clicked with projectId', async () => {
    const { useProject } = await import('@/contexts/ProjectContext');
    (useProject as Mock).mockReturnValue({
      projectId: 'my-project',
      project: { id: 'my-project', name: 'My Project' },
    });
    render(<BottomNav />);
    const tasksBtn = screen.getByRole('button', { name: /tasks/i });
    fireEvent.click(tasksBtn);
    expect(mockNavigate).toHaveBeenCalledWith('/projects/my-project/tasks');
  });
});
