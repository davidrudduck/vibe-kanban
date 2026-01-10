import { useMemo } from 'react';
import { Variable, AlertTriangle, Link2 } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from '@/components/ui/popover';
import { useResolvedVariables } from '@/hooks';
import { cn } from '@/lib/utils';

interface VariablesBadgeProps {
  taskId: string | undefined;
  taskDescription?: string | null;
  className?: string;
}

/**
 * Compact badge showing variable count with popover for full list.
 * Designed for the toolbar above the input area.
 * Shows warning state when undefined variables exist.
 */
export function VariablesBadge({
  taskId,
  taskDescription,
  className,
}: VariablesBadgeProps) {
  const { data: resolvedVariables = [] } = useResolvedVariables(taskId);

  // Extract undefined variables from the task description
  const undefinedVariables = useMemo(() => {
    if (!taskDescription) return [];

    const varPattern = /\$\{([A-Z][A-Z0-9_]*)\}|\$([A-Z][A-Z0-9_]*)/g;
    const referencedVars = new Set<string>();
    let match;
    while ((match = varPattern.exec(taskDescription)) !== null) {
      referencedVars.add(match[1] || match[2]);
    }

    const definedNames = new Set(resolvedVariables.map((v) => v.name));
    return Array.from(referencedVars).filter((name) => !definedNames.has(name));
  }, [taskDescription, resolvedVariables]);

  const hasWarnings = undefinedVariables.length > 0;
  const totalCount = resolvedVariables.length;
  const inheritedCount = resolvedVariables.filter((v) => v.inherited).length;

  // Don't render if no task
  if (!taskId) return null;

  return (
    <Popover>
      <PopoverTrigger asChild>
        <Button
          variant="ghost"
          size="sm"
          className={cn(
            'h-8 px-2 text-xs font-medium tabular-nums min-h-[44px] min-w-[44px]',
            hasWarnings && 'text-warning',
            totalCount === 0 && !hasWarnings && 'text-muted-foreground',
            className
          )}
          aria-label={`${totalCount} variables${hasWarnings ? `, ${undefinedVariables.length} undefined` : ''}`}
        >
          <span className="flex items-center gap-1">
            <Variable className="h-3.5 w-3.5" aria-hidden />
            {hasWarnings && <AlertTriangle className="h-3 w-3" aria-hidden />}
            <span className="hidden sm:inline ml-1">Vars</span>
            <span className="font-mono text-xs">({totalCount})</span>
          </span>
        </Button>
      </PopoverTrigger>
      <PopoverContent className="w-72 sm:w-80 p-0" align="start" sideOffset={4}>
        <div className="px-3 py-2 border-b flex items-center justify-between">
          <h4 className="text-sm font-medium">Variables ({totalCount})</h4>
          {inheritedCount > 0 && (
            <Badge variant="secondary" className="text-[10px] px-1.5 py-0">
              {inheritedCount} inherited
            </Badge>
          )}
        </div>

        {/* Undefined variables warning */}
        {undefinedVariables.length > 0 && (
          <div className="mx-2 mt-2 p-2 rounded-md border border-warning/50 bg-warning/10">
            <div className="flex items-start gap-2">
              <AlertTriangle className="h-4 w-4 text-warning shrink-0 mt-0.5" />
              <div className="space-y-1">
                <p className="text-xs font-medium text-warning">
                  Undefined Variables
                </p>
                <div className="flex flex-wrap gap-1">
                  {undefinedVariables.map((name) => (
                    <code
                      key={name}
                      className="text-[10px] bg-muted px-1 py-0.5 rounded font-mono"
                    >
                      ${name}
                    </code>
                  ))}
                </div>
              </div>
            </div>
          </div>
        )}

        {totalCount === 0 && undefinedVariables.length === 0 ? (
          <p className="text-sm text-muted-foreground text-center py-4 px-3">
            No variables defined.
          </p>
        ) : totalCount > 0 ? (
          <ul className="max-h-64 overflow-y-auto p-2 space-y-1">
            {resolvedVariables.map((variable) => (
              <li
                key={variable.name}
                className="flex items-start gap-2 p-1 rounded hover:bg-muted/50"
              >
                <code className="text-xs font-mono text-foreground shrink-0 mt-0.5">
                  ${variable.name}
                </code>
                <span className="text-xs text-muted-foreground flex-1 min-w-0 truncate">
                  {variable.value || <span className="italic">(empty)</span>}
                </span>
                {variable.inherited && (
                  <Badge
                    variant="outline"
                    className="text-[10px] px-1 py-0 shrink-0 flex items-center gap-0.5"
                  >
                    <Link2 className="h-2.5 w-2.5" aria-hidden />
                  </Badge>
                )}
              </li>
            ))}
          </ul>
        ) : null}
      </PopoverContent>
    </Popover>
  );
}

export default VariablesBadge;
