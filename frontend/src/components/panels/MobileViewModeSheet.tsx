import { useTranslation } from 'react-i18next';
import { Eye, FileDiff, FolderTree, Terminal, FileText, Cog } from 'lucide-react';
import { BottomSheet } from '@/components/ui/bottom-sheet';
import { Button } from '@/components/ui/button';
import { cn } from '@/lib/utils';
import type { LayoutMode } from '@/components/layout/TasksLayout';

interface MobileViewModeSheetProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  mode: LayoutMode;
  onModeChange: (mode: LayoutMode) => void;
}

interface ModeOption {
  value: LayoutMode;
  label: string;
  icon: React.ReactNode;
}

/**
 * Mobile bottom sheet for selecting view modes (logs, preview, diffs, files, terminal, processes).
 * Provides touch-friendly 48px buttons for easy selection.
 */
export function MobileViewModeSheet({
  open,
  onOpenChange,
  mode,
  onModeChange,
}: MobileViewModeSheetProps) {
  const { t } = useTranslation('tasks');

  const modes: ModeOption[] = [
    {
      value: null,
      label: t('mobileViewModeSheet.logs', { defaultValue: 'Logs' }),
      icon: <FileText className="h-5 w-5" />,
    },
    {
      value: 'preview',
      label: t('attemptHeaderActions.preview', { defaultValue: 'Preview' }),
      icon: <Eye className="h-5 w-5" />,
    },
    {
      value: 'diffs',
      label: t('attemptHeaderActions.diffs', { defaultValue: 'Diffs' }),
      icon: <FileDiff className="h-5 w-5" />,
    },
    {
      value: 'files',
      label: t('attemptHeaderActions.files', { defaultValue: 'Files' }),
      icon: <FolderTree className="h-5 w-5" />,
    },
    {
      value: 'terminal',
      label: t('attemptHeaderActions.terminal', { defaultValue: 'Terminal' }),
      icon: <Terminal className="h-5 w-5" />,
    },
    {
      value: 'processes',
      label: t('attemptHeaderActions.processes', { defaultValue: 'Processes' }),
      icon: <Cog className="h-5 w-5" />,
    },
  ];

  const handleModeSelect = (newMode: LayoutMode) => {
    onModeChange(newMode);
    onOpenChange(false);
  };

  return (
    <BottomSheet
      open={open}
      onOpenChange={onOpenChange}
      title={t('mobileViewModeSheet.title', { defaultValue: 'View Mode' })}
    >
      <div className="flex flex-col gap-1">
        {modes.map((option) => {
          const isActive = mode === option.value;
          return (
            <Button
              key={option.value ?? 'logs'}
              variant="ghost"
              className={cn(
                'w-full justify-start gap-3 h-12 min-h-[48px] px-4 py-3',
                isActive && 'bg-accent text-accent-foreground'
              )}
              onClick={() => handleModeSelect(option.value)}
              aria-label={option.label}
              aria-pressed={isActive}
            >
              {option.icon}
              <span className="text-base">{option.label}</span>
            </Button>
          );
        })}
      </div>
    </BottomSheet>
  );
}
