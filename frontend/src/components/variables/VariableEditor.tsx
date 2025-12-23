import { useState, useCallback, useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Badge } from '@/components/ui/badge';
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from '@/components/ui/tooltip';
import {
  Plus,
  Trash2,
  Edit2,
  Check,
  X,
  Loader2,
  AlertCircle,
} from 'lucide-react';
import {
  useTaskVariables,
  useResolvedVariables,
  useTaskVariableMutations,
} from '@/hooks';
import type { ResolvedVariable } from 'shared/types';
import { cn } from '@/lib/utils';

// Regex for valid variable names: starts with uppercase, followed by uppercase/digits/underscores
const VARIABLE_NAME_REGEX = /^[A-Z][A-Z0-9_]*$/;

type Props = {
  taskId: string;
  disabled?: boolean;
  /** Show inherited variables from parent tasks */
  showInherited?: boolean;
  /** Compact layout for embedding in forms */
  compact?: boolean;
  className?: string;
};

type EditingState = {
  id: string | null;
  name: string;
  value: string;
  isNew: boolean;
};

function VariableEditor({
  taskId,
  disabled = false,
  showInherited = true,
  compact = false,
  className,
}: Props) {
  const { t } = useTranslation('tasks');

  // Fetch task's own variables
  const { data: ownVariables = [], isLoading: loadingOwn } =
    useTaskVariables(taskId);

  // Fetch resolved variables (includes inherited)
  const { data: resolvedVariables = [], isLoading: loadingResolved } =
    useResolvedVariables(showInherited ? taskId : undefined);

  // Mutations
  const { createVariable, updateVariable, deleteVariable } =
    useTaskVariableMutations(taskId);

  // Editing state
  const [editing, setEditing] = useState<EditingState | null>(null);
  const [error, setError] = useState<string | null>(null);

  // Loading states
  const isLoading = loadingOwn || loadingResolved;
  const isMutating =
    createVariable.isPending ||
    updateVariable.isPending ||
    deleteVariable.isPending;

  // Combine own and inherited variables for display
  const displayVariables = useMemo(() => {
    if (!showInherited) {
      // Just show own variables as ResolvedVariable format
      return ownVariables.map((v) => ({
        name: v.name,
        value: v.value,
        source_task_id: v.task_id,
        inherited: false,
        id: v.id,
      }));
    }

    // Map resolved variables with IDs from own variables for editing
    return resolvedVariables.map((rv) => {
      const ownVar = ownVariables.find((ov) => ov.name === rv.name);
      return {
        ...rv,
        id: ownVar?.id,
      };
    });
  }, [ownVariables, resolvedVariables, showInherited]);

  // Start creating a new variable
  const handleStartNew = useCallback(() => {
    setEditing({
      id: null,
      name: '',
      value: '',
      isNew: true,
    });
    setError(null);
  }, []);

  // Start editing an existing variable
  const handleStartEdit = useCallback(
    (variable: { id?: string; name: string; value: string }) => {
      if (!variable.id) return; // Can't edit inherited variables without overriding
      setEditing({
        id: variable.id,
        name: variable.name,
        value: variable.value,
        isNew: false,
      });
      setError(null);
    },
    []
  );

  // Cancel editing
  const handleCancel = useCallback(() => {
    setEditing(null);
    setError(null);
  }, []);

  // Validate variable name
  const validateName = useCallback(
    (name: string): string | null => {
      if (!name.trim()) {
        return t('variables.errors.nameRequired', 'Variable name is required');
      }
      if (!VARIABLE_NAME_REGEX.test(name)) {
        return t(
          'variables.errors.invalidName',
          'Name must start with uppercase letter, contain only A-Z, 0-9, and underscores'
        );
      }
      // Check for duplicates (only for new variables or name changes)
      if (editing?.isNew || editing?.name !== name) {
        const exists = ownVariables.some((v) => v.name === name);
        if (exists) {
          return t(
            'variables.errors.nameExists',
            'A variable with this name already exists'
          );
        }
      }
      return null;
    },
    [editing, ownVariables, t]
  );

  // Save the variable
  const handleSave = useCallback(async () => {
    if (!editing) return;

    const nameError = validateName(editing.name);
    if (nameError) {
      setError(nameError);
      return;
    }

    try {
      if (editing.isNew) {
        await createVariable.mutateAsync({
          name: editing.name,
          value: editing.value,
        });
      } else if (editing.id) {
        await updateVariable.mutateAsync({
          variableId: editing.id,
          data: {
            name: editing.name,
            value: editing.value,
          },
        });
      }
      setEditing(null);
      setError(null);
    } catch (err) {
      console.error('Failed to save variable:', err);
      setError(t('variables.errors.saveFailed', 'Failed to save variable'));
    }
  }, [editing, validateName, createVariable, updateVariable, t]);

  // Delete a variable
  const handleDelete = useCallback(
    async (variable: { id?: string; name: string }) => {
      if (!variable.id) return;

      if (
        !confirm(
          t('variables.deleteConfirm', {
            name: variable.name,
            defaultValue: `Delete variable "${variable.name}"?`,
          })
        )
      ) {
        return;
      }

      try {
        await deleteVariable.mutateAsync(variable.id);
      } catch (err) {
        console.error('Failed to delete variable:', err);
      }
    },
    [deleteVariable, t]
  );

  // Override an inherited variable
  const handleOverride = useCallback((variable: ResolvedVariable) => {
    setEditing({
      id: null,
      name: variable.name,
      value: variable.value,
      isNew: true, // This creates a new variable that shadows the inherited one
    });
    setError(null);
  }, []);

  // Handle Enter key in inputs
  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === 'Enter' && !e.shiftKey) {
        e.preventDefault();
        handleSave();
      } else if (e.key === 'Escape') {
        handleCancel();
      }
    },
    [handleSave, handleCancel]
  );

  if (isLoading) {
    return (
      <div className={cn('flex items-center justify-center py-4', className)}>
        <Loader2 className="h-5 w-5 animate-spin text-muted-foreground" />
      </div>
    );
  }

  return (
    <div className={cn('space-y-2', className)}>
      {/* Header */}
      <div className="flex items-center justify-between">
        <h4 className="text-sm font-medium text-muted-foreground">
          {t('variables.title', 'Variables')}
        </h4>
        <Button
          variant="ghost"
          size="sm"
          onClick={handleStartNew}
          disabled={disabled || isMutating || editing !== null}
          className="h-7 px-2 text-xs"
        >
          <Plus className="h-3 w-3 mr-1" />
          {t('variables.add', 'Add')}
        </Button>
      </div>

      {/* Variable list */}
      <div
        className={cn(
          'border rounded-md overflow-hidden',
          compact ? 'max-h-48' : 'max-h-64',
          'overflow-y-auto'
        )}
      >
        {displayVariables.length === 0 && !editing ? (
          <div className="text-center py-4 text-sm text-muted-foreground">
            {t('variables.empty', 'No variables defined')}
          </div>
        ) : (
          <div className="divide-y">
            {/* New variable row */}
            {editing?.isNew && (
              <div className="p-2 bg-muted/30">
                <div className="flex items-start gap-2">
                  <div className="flex-1 space-y-2">
                    <div className="flex gap-2">
                      <Input
                        value={editing.name}
                        onChange={(e) =>
                          setEditing({
                            ...editing,
                            name: e.target.value.toUpperCase(),
                          })
                        }
                        onKeyDown={handleKeyDown}
                        placeholder={t(
                          'variables.namePlaceholder',
                          'VARIABLE_NAME'
                        )}
                        className="h-8 text-sm font-mono flex-1"
                        disabled={isMutating}
                        autoFocus
                      />
                    </div>
                    <Input
                      value={editing.value}
                      onChange={(e) =>
                        setEditing({ ...editing, value: e.target.value })
                      }
                      onKeyDown={handleKeyDown}
                      placeholder={t('variables.valuePlaceholder', 'Value')}
                      className="h-8 text-sm"
                      disabled={isMutating}
                    />
                    {error && (
                      <div className="flex items-center gap-1 text-xs text-destructive">
                        <AlertCircle className="h-3 w-3" />
                        {error}
                      </div>
                    )}
                  </div>
                  <div className="flex gap-1 pt-1">
                    <Button
                      variant="ghost"
                      size="icon"
                      className="h-6 w-6"
                      onClick={handleSave}
                      disabled={isMutating}
                    >
                      {isMutating ? (
                        <Loader2 className="h-3 w-3 animate-spin" />
                      ) : (
                        <Check className="h-3 w-3 text-green-600 dark:text-green-400" />
                      )}
                    </Button>
                    <Button
                      variant="ghost"
                      size="icon"
                      className="h-6 w-6"
                      onClick={handleCancel}
                      disabled={isMutating}
                    >
                      <X className="h-3 w-3" />
                    </Button>
                  </div>
                </div>
              </div>
            )}

            {/* Existing variables */}
            {displayVariables.map((variable) => (
              <div
                key={variable.name}
                className={cn('p-2 group', variable.inherited && 'bg-muted/20')}
              >
                {editing && editing.id === variable.id ? (
                  // Editing existing variable
                  <div className="flex items-start gap-2">
                    <div className="flex-1 space-y-2">
                      <div className="flex gap-2">
                        <Input
                          value={editing.name}
                          onChange={(e) =>
                            setEditing((prev) =>
                              prev
                                ? {
                                    ...prev,
                                    name: e.target.value.toUpperCase(),
                                  }
                                : null
                            )
                          }
                          onKeyDown={handleKeyDown}
                          className="h-8 text-sm font-mono flex-1"
                          disabled={isMutating}
                          autoFocus
                        />
                      </div>
                      <Input
                        value={editing.value}
                        onChange={(e) =>
                          setEditing((prev) =>
                            prev ? { ...prev, value: e.target.value } : null
                          )
                        }
                        onKeyDown={handleKeyDown}
                        className="h-8 text-sm"
                        disabled={isMutating}
                      />
                      {error && (
                        <div className="flex items-center gap-1 text-xs text-destructive">
                          <AlertCircle className="h-3 w-3" />
                          {error}
                        </div>
                      )}
                    </div>
                    <div className="flex gap-1 pt-1">
                      <Button
                        variant="ghost"
                        size="icon"
                        className="h-6 w-6"
                        onClick={handleSave}
                        disabled={isMutating}
                      >
                        {isMutating ? (
                          <Loader2 className="h-3 w-3 animate-spin" />
                        ) : (
                          <Check className="h-3 w-3 text-green-600 dark:text-green-400" />
                        )}
                      </Button>
                      <Button
                        variant="ghost"
                        size="icon"
                        className="h-6 w-6"
                        onClick={handleCancel}
                        disabled={isMutating}
                      >
                        <X className="h-3 w-3" />
                      </Button>
                    </div>
                  </div>
                ) : (
                  // Display mode
                  <div className="flex items-center gap-2">
                    <div className="flex-1 min-w-0">
                      <div className="flex items-center gap-2">
                        <code className="text-sm font-mono text-foreground">
                          ${variable.name}
                        </code>
                        {variable.inherited && (
                          <Tooltip>
                            <TooltipTrigger asChild>
                              <Badge
                                variant="secondary"
                                className="text-[10px] px-1.5 py-0"
                              >
                                {t('variables.inherited', 'Inherited')}
                              </Badge>
                            </TooltipTrigger>
                            <TooltipContent>
                              {t(
                                'variables.inheritedTooltip',
                                'From parent task'
                              )}
                            </TooltipContent>
                          </Tooltip>
                        )}
                      </div>
                      <div
                        className="text-sm text-muted-foreground truncate mt-0.5"
                        title={variable.value}
                      >
                        {variable.value || (
                          <span className="italic">
                            {t('variables.emptyValue', '(empty)')}
                          </span>
                        )}
                      </div>
                    </div>
                    <div
                      className={cn(
                        'flex gap-1',
                        'opacity-0 group-hover:opacity-100 transition-opacity',
                        disabled && 'hidden'
                      )}
                    >
                      {variable.inherited ? (
                        // Override button for inherited variables
                        <Tooltip>
                          <TooltipTrigger asChild>
                            <Button
                              variant="ghost"
                              size="icon"
                              className="h-6 w-6"
                              onClick={() =>
                                handleOverride(variable as ResolvedVariable)
                              }
                              disabled={isMutating || editing !== null}
                            >
                              <Edit2 className="h-3 w-3" />
                            </Button>
                          </TooltipTrigger>
                          <TooltipContent>
                            {t('variables.override', 'Override')}
                          </TooltipContent>
                        </Tooltip>
                      ) : (
                        // Edit and delete buttons for own variables
                        <>
                          <Button
                            variant="ghost"
                            size="icon"
                            className="h-6 w-6"
                            onClick={() => handleStartEdit(variable)}
                            disabled={isMutating || editing !== null}
                          >
                            <Edit2 className="h-3 w-3" />
                          </Button>
                          <Button
                            variant="ghost"
                            size="icon"
                            className="h-6 w-6"
                            onClick={() => handleDelete(variable)}
                            disabled={isMutating || editing !== null}
                          >
                            <Trash2 className="h-3 w-3" />
                          </Button>
                        </>
                      )}
                    </div>
                  </div>
                )}
              </div>
            ))}
          </div>
        )}
      </div>

      {/* Hint text */}
      {!compact && (
        <p className="text-xs text-muted-foreground">
          {t(
            'variables.hint',
            'Variables can be used in task descriptions with $VAR_NAME or ${VAR_NAME} syntax.'
          )}
        </p>
      )}
    </div>
  );
}

export default VariableEditor;
