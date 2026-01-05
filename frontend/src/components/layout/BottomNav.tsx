import { useLocation, useNavigate } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import { FolderOpen, ListTodo, Plus, Bell, Menu } from 'lucide-react';
import { cn } from '@/lib/utils';
import { useProject } from '@/contexts/ProjectContext';
import { openTaskForm } from '@/lib/openTaskForm';

interface NavItemProps {
  icon: React.ElementType;
  label: string;
  isActive: boolean;
  onClick: () => void;
}

function NavItem({ icon: Icon, label, isActive, onClick }: NavItemProps) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={cn(
        'flex flex-col items-center justify-center gap-0.5 h-12 min-w-[48px] px-3 rounded-lg transition-colors',
        isActive
          ? 'text-foreground font-medium'
          : 'text-muted-foreground hover:text-foreground'
      )}
      aria-label={label}
      aria-current={isActive ? 'page' : undefined}
    >
      <Icon className="h-5 w-5" />
      <span className="text-[10px] font-medium">{label}</span>
    </button>
  );
}

export function BottomNav() {
  const location = useLocation();
  const navigate = useNavigate();
  const { t } = useTranslation('common');
  const { projectId } = useProject();

  const isProjectsActive =
    location.pathname === '/projects' ||
    (location.pathname.startsWith('/projects/') &&
      !location.pathname.includes('/tasks'));

  const isTasksActive =
    location.pathname.includes('/tasks') || location.pathname === '/tasks/all';

  const handleProjectsClick = () => {
    navigate('/projects');
  };

  const handleTasksClick = () => {
    if (projectId) {
      navigate(`/projects/${projectId}/tasks`);
    } else {
      navigate('/tasks/all');
    }
  };

  const handleAddClick = () => {
    if (projectId) {
      openTaskForm({ mode: 'create', projectId });
    }
  };

  const handleActivityClick = () => {
    // Activity is typically handled via the ActivityFeed component
    // For now, we'll navigate to processes page which shows activity
    navigate('/processes');
  };

  const handleMenuClick = () => {
    // Menu is handled by opening the settings page
    navigate('/settings');
  };

  return (
    <nav
      role="navigation"
      aria-label={t('bottomNav.ariaLabel', 'Main navigation')}
      className="fixed bottom-0 left-0 right-0 z-50 h-14 bg-background border-t border-border sm:hidden safe-area-bottom"
    >
      <div className="flex items-center justify-around h-full max-w-md mx-auto px-2">
        <NavItem
          icon={FolderOpen}
          label={t('bottomNav.projects', 'Projects')}
          isActive={isProjectsActive}
          onClick={handleProjectsClick}
        />
        <NavItem
          icon={ListTodo}
          label={t('bottomNav.tasks', 'Tasks')}
          isActive={isTasksActive}
          onClick={handleTasksClick}
        />
        <NavItem
          icon={Plus}
          label={t('bottomNav.add', 'Add')}
          isActive={false}
          onClick={handleAddClick}
        />
        <NavItem
          icon={Bell}
          label={t('bottomNav.activity', 'Activity')}
          isActive={location.pathname === '/processes'}
          onClick={handleActivityClick}
        />
        <NavItem
          icon={Menu}
          label={t('bottomNav.menu', 'Menu')}
          isActive={location.pathname.startsWith('/settings')}
          onClick={handleMenuClick}
        />
      </div>
    </nav>
  );
}
