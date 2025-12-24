import { cn } from '@/lib/utils';
import { Wifi, WifiOff, Cloud, Monitor } from 'lucide-react';
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from '@/components/ui/tooltip';

export type ConnectionStatus = 'local' | 'direct' | 'relay' | 'disconnected';

interface ConnectionStatusBadgeProps {
  status: ConnectionStatus;
  className?: string;
  /** Optional tooltip text override */
  tooltip?: string;
}

const statusConfig: Record<
  ConnectionStatus,
  {
    label: string;
    icon: typeof Wifi;
    bgColor: string;
    textColor: string;
    iconColor: string;
    tooltip: string;
  }
> = {
  local: {
    label: 'Local',
    icon: Monitor,
    bgColor: 'bg-slate-100 dark:bg-slate-800',
    textColor: 'text-slate-700 dark:text-slate-300',
    iconColor: 'text-slate-500',
    tooltip: 'Running on this node',
  },
  direct: {
    label: 'Direct',
    icon: Wifi,
    bgColor: 'bg-green-100 dark:bg-green-900/30',
    textColor: 'text-green-800 dark:text-green-300',
    iconColor: 'text-green-500',
    tooltip: 'Connected directly to remote node',
  },
  relay: {
    label: 'Relay',
    icon: Cloud,
    bgColor: 'bg-blue-100 dark:bg-blue-900/30',
    textColor: 'text-blue-800 dark:text-blue-300',
    iconColor: 'text-blue-500',
    tooltip: 'Connected via hive relay',
  },
  disconnected: {
    label: 'Disconnected',
    icon: WifiOff,
    bgColor: 'bg-red-100 dark:bg-red-900/30',
    textColor: 'text-red-800 dark:text-red-300',
    iconColor: 'text-red-500',
    tooltip: 'Unable to connect to remote node',
  },
};

export function ConnectionStatusBadge({
  status,
  className,
  tooltip,
}: ConnectionStatusBadgeProps) {
  const config = statusConfig[status];
  const Icon = config.icon;

  return (
    <TooltipProvider>
      <Tooltip>
        <TooltipTrigger asChild>
          <span
            className={cn(
              'inline-flex items-center gap-1.5 px-2 py-0.5 rounded-full text-xs font-medium cursor-default',
              config.bgColor,
              config.textColor,
              className
            )}
          >
            <Icon
              className={cn('w-3 h-3', config.iconColor)}
              aria-hidden="true"
            />
            {config.label}
          </span>
        </TooltipTrigger>
        <TooltipContent side="bottom">
          {tooltip || config.tooltip}
        </TooltipContent>
      </Tooltip>
    </TooltipProvider>
  );
}
