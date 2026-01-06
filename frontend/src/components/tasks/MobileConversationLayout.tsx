import * as React from 'react';
import { useState } from 'react';
import { useEntries } from '@/contexts/EntriesContext';
import { usePinnedTodos } from '@/hooks/usePinnedTodos';
import { useMessageQueue } from '@/hooks/message-queue';
import { TodosBadge } from './TodosBadge';
import { MessageQueueBadge } from './message-queue';
import { TaskInfoSheet } from './TaskInfoSheet';
import type { TaskWithAttemptStatus } from 'shared/types';

interface MobileConversationLayoutProps {
  task: TaskWithAttemptStatus;
  logs: React.ReactNode;
  followUp: React.ReactNode;
  relationships: React.ReactNode;
  variables: React.ReactNode;
  isMobile: boolean;
  selectedAttemptId?: string;
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
  selectedAttemptId,
}: MobileConversationLayoutProps) {
  const [isTaskInfoOpen, setIsTaskInfoOpen] = useState(false);
  const { entries } = useEntries();
  const { todos } = usePinnedTodos(entries);

  // Message queue data for the toolbar badge
  const {
    queue: messageQueue,
    isLoading: isMessageQueueLoading,
    updateMessage: updateQueuedMessage,
    removeMessage: removeFromQueue,
    reorderMessages: reorderQueue,
    clearQueue,
    isRemoving: isRemovingFromQueue,
    isClearing: isClearingQueue,
  } = useMessageQueue(selectedAttemptId);

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

        {/* Compact toolbar with badges */}
        <div
          className="shrink-0 border-t px-2 py-1 flex items-center justify-between bg-background"
          role="toolbar"
          aria-label="Task toolbar"
        >
          <div className="flex items-center gap-1">
            <TodosBadge todos={todos} />
            <MessageQueueBadge
              queue={messageQueue}
              isLoading={isMessageQueueLoading}
              onUpdate={async (messageId, content) => {
                await updateQueuedMessage(messageId, content);
              }}
              onRemove={removeFromQueue}
              onReorder={async (ids) => {
                await reorderQueue(ids);
              }}
              onClear={clearQueue}
              isRemoving={isRemovingFromQueue}
              isClearing={isClearingQueue}
            />
          </div>
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

  // Desktop layout - same toolbar pattern as mobile but with more space
  return (
    <div className="flex-1 min-h-0 flex flex-col">
      {/* Task Info Sheet (overlay) - shared with mobile */}
      <TaskInfoSheet
        task={task}
        isOpen={isTaskInfoOpen}
        onOpenChange={setIsTaskInfoOpen}
        relationships={relationships}
        variables={variables}
      />

      {/* Task Relationships - shown when parent/child tasks exist */}
      {relationships && (
        <div className="shrink-0 border-b">
          <div className="mx-auto w-full max-w-[50rem]">{relationships}</div>
        </div>
      )}

      {/* Conversation area */}
      <div className="flex-1 min-h-0 flex flex-col">{logs}</div>

      {/* Compact toolbar with badges - same as mobile */}
      <div className="shrink-0 border-t">
        <div className="mx-auto w-full max-w-[50rem]">
          <div
            className="px-2 py-1 flex items-center justify-between bg-background"
            role="toolbar"
            aria-label="Task toolbar"
          >
            <div className="flex items-center gap-1">
              <TodosBadge todos={todos} />
              <MessageQueueBadge
                queue={messageQueue}
                isLoading={isMessageQueueLoading}
                onUpdate={async (messageId, content) => {
                  await updateQueuedMessage(messageId, content);
                }}
                onRemove={removeFromQueue}
                onReorder={async (ids) => {
                  await reorderQueue(ids);
                }}
                onClear={clearQueue}
                isRemoving={isRemovingFromQueue}
                isClearing={isClearingQueue}
              />
            </div>
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
        </div>
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
