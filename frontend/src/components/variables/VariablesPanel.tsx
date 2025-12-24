import { useState, useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import { ChevronUp, Variable, AlertTriangle, Link2 } from 'lucide-react';
import { Card } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from '@/components/ui/tooltip';
import { useResolvedVariables } from '@/hooks';
import type { ResolvedVariable } from 'shared/types';

const VARIABLES_PANEL_OPEN_KEY = 'variables-panel-open';

type Props = {
  /** Task ID to show variables for */
  taskId: string | undefined;
  /** Task description for preview expansion */
  taskDescription?: string;
};

/**
 * Collapsible panel showing task variables with inheritance chain visualization.
 * Designed to be shown alongside TodoPanel in the task attempt view.
 */
function VariablesPanel({ taskId, taskDescription }: Props) {
  const { t } = useTranslation('tasks');

  const [isOpen, setIsOpen] = useState(() => {
    const stored = localStorage.getItem(VARIABLES_PANEL_OPEN_KEY);
    return stored === 'true';
  });

  // Fetch resolved variables (includes inherited)
  const { data: resolvedVariables = [] } = useResolvedVariables(taskId);

  // Extract undefined variables from the task description
  const undefinedVariables = useMemo(() => {
    if (!taskDescription) return [];

    // Extract all variable references from description
    const varPattern = /\$\{([A-Z][A-Z0-9_]*)\}|\$([A-Z][A-Z0-9_]*)/g;
    const referencedVars = new Set<string>();
    let match;
    while ((match = varPattern.exec(taskDescription)) !== null) {
      referencedVars.add(match[1] || match[2]);
    }

    // Find which ones are not defined
    const definedNames = new Set(resolvedVariables.map((v) => v.name));
    return Array.from(referencedVars).filter((name) => !definedNames.has(name));
  }, [taskDescription, resolvedVariables]);

  // Count of inherited vs own variables
  const inheritedCount = resolvedVariables.filter((v) => v.inherited).length;
  const ownCount = resolvedVariables.length - inheritedCount;

  // Don't render if no task or no variables
  if (
    !taskId ||
    (resolvedVariables.length === 0 && undefinedVariables.length === 0)
  ) {
    return null;
  }

  const hasWarnings = undefinedVariables.length > 0;

  return (
    <details
      className="group"
      open={isOpen}
      onToggle={(e) => {
        const newState = e.currentTarget.open;
        setIsOpen(newState);
        localStorage.setItem(VARIABLES_PANEL_OPEN_KEY, String(newState));
      }}
    >
      <summary className="list-none cursor-pointer">
        <Card className="bg-muted p-3 text-sm flex items-center justify-between">
          <span className="flex items-center gap-2">
            <Variable className="h-4 w-4 text-muted-foreground" />
            {t('variables.panelTitle', {
              count: resolvedVariables.length,
              defaultValue: 'Variables ({{count}})',
            })}
            {inheritedCount > 0 && (
              <Badge variant="secondary" className="text-[10px] px-1.5 py-0">
                {t('variables.inheritedCount', {
                  count: inheritedCount,
                  defaultValue: '{{count}} inherited',
                })}
              </Badge>
            )}
            {hasWarnings && (
              <TooltipProvider>
                <Tooltip>
                  <TooltipTrigger asChild>
                    <AlertTriangle className="h-4 w-4 text-warning" />
                  </TooltipTrigger>
                  <TooltipContent>
                    {t('variables.undefinedWarning', {
                      count: undefinedVariables.length,
                      defaultValue: '{{count}} undefined variable(s)',
                    })}
                  </TooltipContent>
                </Tooltip>
              </TooltipProvider>
            )}
          </span>
          <ChevronUp
            aria-hidden
            className="h-4 w-4 text-muted-foreground transition-transform group-open:rotate-180"
          />
        </Card>
      </summary>

      <div className="px-3 pb-2 space-y-3">
        {/* Undefined variables warning */}
        {undefinedVariables.length > 0 && (
          <div className="mt-2 p-2 rounded-md border border-warning/50 bg-warning/10">
            <div className="flex items-start gap-2">
              <AlertTriangle className="h-4 w-4 text-warning shrink-0 mt-0.5" />
              <div className="space-y-1">
                <p className="text-sm font-medium text-warning">
                  {t('variables.undefinedTitle', 'Undefined Variables')}
                </p>
                <p className="text-xs text-muted-foreground">
                  {t(
                    'variables.undefinedDescription',
                    'These variables are referenced in the task description but not defined:'
                  )}
                </p>
                <div className="flex flex-wrap gap-1 mt-1">
                  {undefinedVariables.map((name) => (
                    <code
                      key={name}
                      className="text-xs bg-muted px-1.5 py-0.5 rounded font-mono"
                    >
                      ${name}
                    </code>
                  ))}
                </div>
              </div>
            </div>
          </div>
        )}

        {/* Variables list with inheritance chain */}
        {resolvedVariables.length > 0 && (
          <div className="mt-2 space-y-2">
            {/* Own variables first */}
            {ownCount > 0 && (
              <div className="space-y-1">
                <h4 className="text-xs font-medium text-muted-foreground uppercase tracking-wide">
                  {t('variables.ownVariables', 'This Task')}
                </h4>
                <ul className="space-y-1">
                  {resolvedVariables
                    .filter((v) => !v.inherited)
                    .map((variable) => (
                      <VariableRow key={variable.name} variable={variable} />
                    ))}
                </ul>
              </div>
            )}

            {/* Inherited variables grouped by source */}
            {inheritedCount > 0 && (
              <div className="space-y-1">
                <h4 className="text-xs font-medium text-muted-foreground uppercase tracking-wide flex items-center gap-1">
                  <Link2 className="h-3 w-3" />
                  {t('variables.inheritedVariables', 'Inherited')}
                </h4>
                <ul className="space-y-1">
                  {resolvedVariables
                    .filter((v) => v.inherited)
                    .map((variable) => (
                      <VariableRow key={variable.name} variable={variable} />
                    ))}
                </ul>
              </div>
            )}
          </div>
        )}

        {/* Hint text */}
        <p className="text-xs text-muted-foreground mt-2">
          {t(
            'variables.panelHint',
            'Variables are expanded when the task is sent to an executor.'
          )}
        </p>
      </div>
    </details>
  );
}

/**
 * Single variable row in the panel
 */
function VariableRow({ variable }: { variable: ResolvedVariable }) {
  const { t } = useTranslation('tasks');

  return (
    <li className="flex items-start gap-2 py-1 px-2 rounded hover:bg-muted/50 transition-colors">
      <code className="text-sm font-mono text-foreground shrink-0">
        ${variable.name}
      </code>
      <span className="text-sm text-muted-foreground flex-1 min-w-0 truncate">
        {variable.value || (
          <span className="italic">{t('variables.emptyValue', '(empty)')}</span>
        )}
      </span>
      {variable.inherited && (
        <Badge variant="outline" className="text-[10px] px-1 py-0 shrink-0">
          {t('variables.inherited', 'Inherited')}
        </Badge>
      )}
    </li>
  );
}

export default VariablesPanel;
