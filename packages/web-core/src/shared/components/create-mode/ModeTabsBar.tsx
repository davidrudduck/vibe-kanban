import type { Icon as PhosphorIcon } from '@phosphor-icons/react';
import { LightningIcon, LinkIcon, PlusIcon } from '@phosphor-icons/react';
import { cn } from '@/shared/lib/utils';
import type { CreateWorkspaceMode } from '@/shared/types/createMode';

interface Tab {
  mode: CreateWorkspaceMode;
  label: string;
  Icon: PhosphorIcon;
}

const TABS: Tab[] = [
  { mode: 'new_task', label: 'New Task', Icon: PlusIcon },
  { mode: 'link_task', label: 'Link Task', Icon: LinkIcon },
  { mode: 'quick_run', label: 'Quick Run', Icon: LightningIcon },
];

interface ModeTabsBarProps {
  mode: CreateWorkspaceMode;
  onChange: (mode: CreateWorkspaceMode) => void;
  disabled?: boolean;
}

export function ModeTabsBar({ mode, onChange, disabled }: ModeTabsBarProps) {
  return (
    <div
      role="tablist"
      aria-label="Workspace creation mode"
      className="flex items-center gap-px rounded-sm border border-border/60 bg-secondary p-quarter"
    >
      {TABS.map(({ mode: tabMode, label, Icon }) => {
        const isActive = mode === tabMode;
        return (
          <button
            key={tabMode}
            type="button"
            role="tab"
            onClick={() => onChange(tabMode)}
            disabled={disabled}
            aria-selected={isActive}
            className={cn(
              'flex flex-1 items-center justify-center gap-1.5 rounded-sm px-3 py-1.5 text-sm font-medium transition-colors',
              'disabled:cursor-not-allowed disabled:opacity-50',
              isActive
                ? 'bg-primary text-high shadow-sm'
                : 'text-low hover:bg-panel hover:text-normal'
            )}
          >
            <Icon
              className={cn(
                'size-icon-xs',
                isActive ? 'text-brand' : 'text-low'
              )}
              weight="bold"
            />
            <span>{label}</span>
          </button>
        );
      })}
    </div>
  );
}
