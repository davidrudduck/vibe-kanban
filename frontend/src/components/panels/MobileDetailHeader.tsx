import { ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { ArrowLeft, LayoutGrid } from 'lucide-react';
import { Button } from '@/components/ui/button';
import type { LayoutMode } from '@/components/layout/TasksLayout';

interface MobileDetailHeaderProps {
  title: string;
  subtitle?: string;
  onBack: () => void;
  mode?: LayoutMode;
  onModeChange?: (mode: LayoutMode) => void;
  onViewModePress?: () => void;
  actions?: ReactNode;
}

/**
 * Mobile-optimized header for task detail panel.
 * Features a back button, title, and optional view mode selector.
 */
export function MobileDetailHeader({
  title,
  subtitle,
  onBack,
  mode,
  onViewModePress,
  actions,
}: MobileDetailHeaderProps) {
  const { t } = useTranslation('tasks');

  return (
    <div className="flex items-center gap-2 px-3 py-2 border-b bg-background sticky top-0 z-20">
      {/* Back button - 44px touch target */}
      <Button
        variant="ghost"
        size="icon"
        className="h-11 w-11 shrink-0"
        onClick={onBack}
        aria-label={t('common:buttons.back', { defaultValue: 'Back' })}
      >
        <ArrowLeft className="h-5 w-5" />
      </Button>

      {/* Title area - takes remaining space */}
      <div className="flex-1 min-w-0 px-1">
        <h1 className="text-sm font-medium truncate overflow-hidden">
          {title}
        </h1>
        {subtitle && (
          <p className="text-xs text-muted-foreground truncate">{subtitle}</p>
        )}
      </div>

      {/* View mode button - only show when onViewModePress is provided */}
      {onViewModePress && (
        <Button
          variant="ghost"
          size="icon"
          className="h-11 w-11 shrink-0"
          onClick={onViewModePress}
          aria-label={t('mobileDetailHeader.viewMode', {
            defaultValue: 'View mode',
          })}
        >
          <LayoutGrid className="h-5 w-5" />
          {mode && (
            <span className="absolute top-1 right-1 h-2 w-2 rounded-full bg-primary" />
          )}
        </Button>
      )}

      {/* Additional actions slot */}
      {actions}
    </div>
  );
}
