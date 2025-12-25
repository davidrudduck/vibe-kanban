import { X } from 'lucide-react';
import { cn } from '@/lib/utils';
import { getLucideIcon } from './IconPicker';
import type { Label } from 'shared/types';

interface LabelBadgeProps {
  label: Label;
  size?: 'sm' | 'md';
  onClick?: () => void;
  onRemove?: () => void;
  className?: string;
}

// Calculate contrasting text color based on background
function getContrastColor(hexColor: string): string {
  // Remove # if present
  const hex = hexColor.replace('#', '');

  // Convert to RGB
  const r = parseInt(hex.substring(0, 2), 16);
  const g = parseInt(hex.substring(2, 4), 16);
  const b = parseInt(hex.substring(4, 6), 16);

  // Calculate luminance
  const luminance = (0.299 * r + 0.587 * g + 0.114 * b) / 255;

  // Return black or white based on luminance
  return luminance > 0.5 ? '#000000' : '#ffffff';
}

export function LabelBadge({
  label,
  size = 'md',
  onClick,
  onRemove,
  className,
}: LabelBadgeProps) {
  const IconComponent = getLucideIcon(label.icon);
  const textColor = getContrastColor(label.color);

  const sizeClasses = {
    sm: 'text-xs px-1.5 py-0.5 gap-1',
    md: 'text-sm px-2 py-1 gap-1.5',
  };

  const iconSizes = {
    sm: 'h-3 w-3',
    md: 'h-3.5 w-3.5',
  };

  const badge = (
    <span
      className={cn(
        'inline-flex items-center rounded-full font-medium transition-opacity',
        sizeClasses[size],
        onClick && 'cursor-pointer hover:opacity-80',
        className
      )}
      style={{
        backgroundColor: label.color,
        color: textColor,
      }}
      onClick={onClick}
    >
      {IconComponent && <IconComponent className={iconSizes[size]} />}
      <span>{label.name}</span>
      {onRemove && (
        <button
          type="button"
          onClick={(e) => {
            e.stopPropagation();
            onRemove();
          }}
          className="ml-0.5 hover:opacity-70 focus:outline-none"
          aria-label={`Remove ${label.name}`}
        >
          <X className={size === 'sm' ? 'h-2.5 w-2.5' : 'h-3 w-3'} />
        </button>
      )}
    </span>
  );

  return badge;
}
