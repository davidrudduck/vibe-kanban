import * as React from 'react';
import { useState } from 'react';
import { useEntries } from '@/contexts/EntriesContext';
import { usePinnedTodos } from '@/hooks/usePinnedTodos';
import { TodosBadge } from './TodosBadge';
import { TaskInfoSheet } from './TaskInfoSheet';
import TodoPanel from './TodoPanel';
import type { TaskWithAttemptStatus } from 'shared/types';

interface MobileConversationLayoutProps {
  task: TaskWithAttemptStatus;
  logs: React.ReactNode;
  followUp: React.ReactNode;
  relationships: React.ReactNode;
  variables: React.ReactNode;
  isMobile: boolean;
}

/**
 * Unified layout component for both mobile and desktop conversation views.
 *
 * Mobile: Conversation takes majority of viewport.
 *   - Task info collapsed into TaskInfoSheet (swipe-down overlay)
 *   - Todos shown as compact badge
 *   - Follow-up input capped to 40vh
 *
 * Desktop: Falls back to full expanded layout.
 */
export function MobileConversationLayout({
  task,
  logs,
  followUp,
  relationships,
  variables,
  isMobile,
}: MobileConversationLayoutProps) {
  const [isTaskInfoOpen, setIsTaskInfoOpen] = useState(false);
  const { entries } = useEntries();
  const { todos } = usePinnedTodos(entries);

  // Check if there's any info to show in the sheet
  const hasTaskInfo = Boolean(task.description || relationships || variables);

  if (isMobile) {
    return (
      <div className="flex-1 min-h-0 flex flex-col">
        {/* Task Info Sheet (overlay) */}
        <TaskInfoSheet
          task={task}
          isOpen={isTaskInfoOpen}
          onOpenChange={setIsTaskInfoOpen}
          relationships={relationships}
          variables={variables}
        />

        {/* Conversation area - takes majority of space */}
        <div className="flex-1 min-h-0 flex flex-col">{logs}</div>

        {/* Compact toolbar with todos badge */}
        <div className="shrink-0 border-t px-2 py-1 flex items-center justify-between bg-background">
          <TodosBadge todos={todos} />
          {hasTaskInfo && (
            <button
              type="button"
              onClick={() => setIsTaskInfoOpen(true)}
              className="text-xs text-muted-foreground hover:text-foreground px-2 py-1 rounded hover:bg-muted"
            >
              Task Info
            </button>
          )}
        </div>

        {/* Follow-up input - capped height on mobile */}
        <div className="min-h-0 max-h-[40vh] border-t overflow-hidden">
          <div className="mx-auto w-full max-w-[50rem] h-full min-h-0">
            {followUp}
          </div>
        </div>
      </div>
    );
  }

  // Desktop layout - full expanded view
  return (
    <div className="flex-1 min-h-0 flex flex-col">
      {/* Task Relationships - shown when parent/child tasks exist */}
      {relationships && (
        <div className="shrink-0 border-b">
          <div className="mx-auto w-full max-w-[50rem]">{relationships}</div>
        </div>
      )}

      {/* Conversation area */}
      <div className="flex-1 min-h-0 flex flex-col">{logs}</div>

      {/* Todos panel */}
      <div className="shrink-0 border-t">
        <div className="mx-auto w-full max-w-[50rem]">
          <TodoPanel />
        </div>
      </div>

      {/* Variables Panel - shown when task has variables */}
      <div className="shrink-0 border-t">
        <div className="mx-auto w-full max-w-[50rem]">{variables}</div>
      </div>

      {/* Follow-up input */}
      <div className="min-h-0 max-h-[50%] border-t overflow-hidden">
        <div className="mx-auto w-full max-w-[50rem] h-full min-h-0">
          {followUp}
        </div>
      </div>
    </div>
  );
}

export default MobileConversationLayout;
