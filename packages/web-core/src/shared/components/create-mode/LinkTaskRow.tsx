import { useCallback, useContext, useMemo, useState } from 'react';
import { MagnifyingGlassIcon, XIcon } from '@phosphor-icons/react';
import { cn } from '@/shared/lib/utils';
import { ProjectContext } from '@/shared/hooks/useProjectContext';
import { buildWorkspaceCreatePrompt } from '@/shared/lib/workspaceCreateState';
import { useCreateMode } from '@/features/create-mode/model/useCreateMode';
import type { LinkedIssue } from '@/shared/types/createMode';
import type { Issue, ProjectStatus } from 'shared/remote-types';

interface LinkTaskRowProps {
  selectedIssue: LinkedIssue | null;
  onIssueSelect: (issue: LinkedIssue | null) => void;
}

function getStatusName(statusId: string, statuses: ProjectStatus[]): string {
  return statuses.find((s) => s.id === statusId)?.name ?? '';
}

export function LinkTaskRow({
  selectedIssue,
  onIssueSelect,
}: LinkTaskRowProps) {
  const { setMessage } = useCreateMode();
  const projectContext = useContext(ProjectContext);
  const [search, setSearch] = useState('');

  const issues: Issue[] = projectContext?.issues ?? [];
  const statuses: ProjectStatus[] = projectContext?.statuses ?? [];
  const projectId: string | null = projectContext?.projectId ?? null;

  const filteredIssues = useMemo(() => {
    const q = search.trim().toLowerCase();
    if (!q) return issues;
    return issues.filter(
      (issue) =>
        issue.title.toLowerCase().includes(q) ||
        issue.simple_id.toLowerCase().includes(q)
    );
  }, [issues, search]);

  const handleSelect = useCallback(
    (issue: Issue) => {
      if (selectedIssue?.issueId === issue.id) {
        // Deselect
        onIssueSelect(null);
        return;
      }

      if (!projectId) return;

      const linked: LinkedIssue = {
        issueId: issue.id,
        simpleId: issue.simple_id,
        title: issue.title,
        remoteProjectId: projectId,
      };

      onIssueSelect(linked);

      const prompt = buildWorkspaceCreatePrompt(
        issue.title,
        issue.description ?? undefined
      );
      if (prompt) {
        setMessage(prompt);
      }
    },
    [selectedIssue, projectId, onIssueSelect, setMessage]
  );

  const handleClearSelected = useCallback(
    (e: React.MouseEvent) => {
      e.stopPropagation();
      onIssueSelect(null);
    },
    [onIssueSelect]
  );

  // No project context — user is not signed in to a remote project
  if (!projectContext) {
    return (
      <div className="flex flex-col gap-half rounded-sm border border-border/60 bg-secondary px-base py-half">
        <p className="text-sm text-low">
          Sign in to a project to browse and link tasks.
        </p>
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-half">
      {/* Selected issue chip */}
      {selectedIssue && (
        <div className="flex items-center gap-half rounded-sm border border-brand/40 bg-brand/5 px-half py-quarter text-sm">
          <span className="font-mono text-xs text-brand">
            {selectedIssue.simpleId}
          </span>
          <span className="min-w-0 flex-1 truncate text-normal">
            {selectedIssue.title}
          </span>
          <button
            type="button"
            onClick={handleClearSelected}
            aria-label="Remove linked task"
            className="shrink-0 text-low hover:text-error"
          >
            <XIcon className="size-icon-2xs" weight="bold" />
          </button>
        </div>
      )}

      {/* Search input */}
      <div className="relative">
        <MagnifyingGlassIcon className="absolute left-half top-1/2 size-icon-xs -translate-y-1/2 text-low" />
        <input
          type="text"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          placeholder="Search tasks…"
          className={cn(
            'w-full rounded-sm border border-border/60 bg-secondary py-half pl-[28px] pr-half',
            'text-sm text-normal placeholder:text-low',
            'focus:outline-none focus:ring-1 focus:ring-brand/50'
          )}
        />
      </div>

      {/* Issue list */}
      <div className="max-h-[200px] overflow-y-auto rounded-sm border border-border/60">
        {filteredIssues.length === 0 ? (
          <p className="px-base py-half text-sm text-low">
            {issues.length === 0
              ? 'No tasks in this project.'
              : 'No matching tasks.'}
          </p>
        ) : (
          filteredIssues.map((issue) => {
            const isActive = selectedIssue?.issueId === issue.id;
            const statusName = getStatusName(issue.status_id, statuses);

            return (
              <button
                key={issue.id}
                type="button"
                onClick={() => handleSelect(issue)}
                aria-pressed={isActive}
                className={cn(
                  'flex w-full min-w-0 items-center gap-half px-base py-half text-left text-sm',
                  'border-b border-border/40 last:border-b-0',
                  'transition-colors',
                  isActive
                    ? 'bg-brand/10 text-high'
                    : 'hover:bg-panel text-normal'
                )}
              >
                <span className="shrink-0 font-mono text-xs text-low w-[60px] truncate">
                  {issue.simple_id}
                </span>
                <span className="min-w-0 flex-1 truncate">{issue.title}</span>
                {statusName && (
                  <span className="shrink-0 text-xs text-low">
                    {statusName}
                  </span>
                )}
              </button>
            );
          })
        )}
      </div>
    </div>
  );
}
