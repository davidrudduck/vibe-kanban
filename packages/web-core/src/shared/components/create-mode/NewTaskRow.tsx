import { cn } from '@/shared/lib/utils';

interface NewTaskRowProps {
  title: string;
  onTitleChange: (title: string) => void;
  disabled?: boolean;
}

export function NewTaskRow({
  title,
  onTitleChange,
  disabled,
}: NewTaskRowProps) {
  return (
    <div className="flex flex-col gap-quarter">
      <input
        type="text"
        value={title}
        onChange={(e) => onTitleChange(e.target.value)}
        disabled={disabled}
        placeholder="Task title (optional)"
        aria-label="Task title"
        className={cn(
          'w-full rounded-sm border border-border/60 bg-secondary px-half py-half',
          'text-sm text-normal placeholder:text-low',
          'focus:outline-none focus:ring-1 focus:ring-brand/50',
          'disabled:cursor-not-allowed disabled:opacity-50'
        )}
      />
    </div>
  );
}
