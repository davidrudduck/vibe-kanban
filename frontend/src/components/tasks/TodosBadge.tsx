import { Circle, Check, CircleDot } from 'lucide-react';
import { Button } from '@/components/ui/button';
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from '@/components/ui/popover';
import type { TodoItem } from 'shared/types';
import { cn } from '@/lib/utils';

interface TodosBadgeProps {
  todos: TodoItem[];
  className?: string;
}

function getStatusIcon(status?: string) {
  const s = (status || '').toLowerCase();
  if (s === 'completed')
    return <Check aria-hidden className="h-3 w-3 text-success flex-shrink-0" />;
  if (s === 'in_progress' || s === 'in-progress')
    return (
      <CircleDot aria-hidden className="h-3 w-3 text-blue-500 flex-shrink-0" />
    );
  return (
    <Circle
      aria-hidden
      className="h-3 w-3 text-muted-foreground flex-shrink-0"
    />
  );
}

/**
 * Compact badge showing todo count with popover for full list.
 * Designed for the toolbar above the input area.
 * Always renders (even with 0 todos) for consistent layout.
 */
export function TodosBadge({ todos, className }: TodosBadgeProps) {
  const safeList = todos || [];
  const pendingCount = safeList.filter(
    (t) => t.status?.toLowerCase() !== 'completed'
  ).length;
  const inProgressCount = safeList.filter(
    (t) => {
      const s = (t.status || '').toLowerCase();
      return s === 'in_progress' || s === 'in-progress';
    }
  ).length;

  return (
    <Popover>
      <PopoverTrigger asChild>
        <Button
          variant="ghost"
          size="sm"
          className={cn(
            'h-8 px-2 text-xs font-medium tabular-nums min-h-[44px] min-w-[44px]',
            inProgressCount > 0 && 'text-blue-600 dark:text-blue-500',
            className
          )}
          aria-label={`${pendingCount} todos pending`}
        >
          <span className="flex items-center gap-1">
            <CircleDot className="h-3.5 w-3.5" aria-hidden />
            <span className="hidden sm:inline ml-1">Queue</span>
            <span className="font-mono text-xs">({pendingCount})</span>
          </span>
        </Button>
      </PopoverTrigger>
      <PopoverContent className="w-72 sm:w-80 p-0" align="start" sideOffset={4}>
        <div className="px-3 py-2 border-b">
          <h4 className="text-sm font-medium">Todos ({safeList.length})</h4>
        </div>
        {safeList.length === 0 ? (
          <p className="text-sm text-muted-foreground text-center py-4 px-3">
            No todos from the conversation yet.
          </p>
        ) : (
          <ul className="max-h-64 overflow-y-auto p-2 space-y-1">
            {safeList.map((todo, index) => (
              <li
                key={`${todo.content}-${index}`}
                className="flex items-start gap-2 p-1 rounded hover:bg-muted/50"
              >
                <span className="mt-0.5 flex-shrink-0">
                  {getStatusIcon(todo.status)}
                </span>
                <span className="text-xs leading-tight break-words flex-1">
                  {todo.content}
                </span>
              </li>
            ))}
          </ul>
        )}
      </PopoverContent>
    </Popover>
  );
}

export default TodosBadge;
