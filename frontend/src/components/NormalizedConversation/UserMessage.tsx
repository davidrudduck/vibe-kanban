import MarkdownRenderer from '@/components/ui/markdown-renderer';
import { Button } from '@/components/ui/button';
import { Pencil, ChevronDown } from 'lucide-react';
import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useProcessRetry } from '@/hooks/useProcessRetry';
import { TaskAttempt, BaseAgentCapability } from 'shared/types';
import { useUserSystem } from '@/components/ConfigProvider';
import { useDraftStream } from '@/hooks/follow-up/useDraftStream';
import { RetryEditorInline } from './RetryEditorInline';
import { useRetryUi } from '@/contexts/RetryUiContext';
import { cn } from '@/lib/utils';
import { useExpandable } from '@/stores/useExpandableStore';

const USER_MESSAGE_APPEARANCE = {
  border: 'border-green-400/40',
  headerBg: 'bg-green-50 dark:bg-green-950/20',
  headerText: 'text-green-700 dark:text-green-300',
  contentBg: 'bg-green-50 dark:bg-green-950/10',
  contentText: 'text-green-700 dark:text-green-300',
};

const LINE_LIMIT = 5;

const UserMessage = ({
  content,
  executionProcessId,
  taskAttempt,
  executorVariant,
}: {
  content: string;
  executionProcessId?: string;
  taskAttempt?: TaskAttempt;
  executorVariant?: string | null;
}) => {
  const { t } = useTranslation('common');
  const [isEditing, setIsEditing] = useState(false);
  const retryHook = useProcessRetry(taskAttempt);
  const { capabilities } = useUserSystem();
  const attemptId = taskAttempt?.id;
  const { retryDraft } = useDraftStream(attemptId);
  const { activeRetryProcessId, isProcessGreyed } = useRetryUi();

  // Line counting and expand/collapse logic
  const lineCount = content.split('\n').length;
  const needsCollapse = lineCount > LINE_LIMIT;
  const [expanded, toggle] = useExpandable(
    `user-message:${executionProcessId || 'no-id'}`,
    !needsCollapse // Default: expanded if short, collapsed if long
  );

  const canFork = !!(
    taskAttempt?.executor &&
    capabilities?.[taskAttempt.executor]?.includes(
      BaseAgentCapability.SESSION_FORK
    )
  );

  // Enter retry mode: create retry draft; actual editor will render inline
  const startRetry = async () => {
    if (!executionProcessId || !taskAttempt) return;
    setIsEditing(true);
    retryHook?.startRetry(executionProcessId, content).catch(() => {
      // rollback if server call fails
      setIsEditing(false);
    });
  };

  // Exit editing state once draft disappears (sent/cancelled)
  useEffect(() => {
    if (!retryDraft?.retry_process_id) setIsEditing(false);
  }, [retryDraft?.retry_process_id]);

  // On reload or when server provides a retry_draft for this process, show editor
  useEffect(() => {
    if (
      executionProcessId &&
      retryDraft?.retry_process_id &&
      retryDraft.retry_process_id === executionProcessId
    ) {
      setIsEditing(true);
    }
  }, [executionProcessId, retryDraft?.retry_process_id]);

  const showRetryEditor =
    !!executionProcessId &&
    isEditing &&
    activeRetryProcessId === executionProcessId;
  const greyed =
    !!executionProcessId &&
    isProcessGreyed(executionProcessId) &&
    !showRetryEditor;

  const retryState = executionProcessId
    ? retryHook?.getRetryDisabledState(executionProcessId)
    : { disabled: true, reason: 'Missing process id' };
  const disabled = !!retryState?.disabled;
  const reason = retryState?.reason ?? undefined;
  const editTitle = disabled && reason ? reason : 'Edit message';

  const executor = taskAttempt?.executor;
  const variant = executorVariant ?? null;

  // Content truncation for collapsed state
  const displayContent =
    needsCollapse && !expanded
      ? content.split('\n').slice(0, LINE_LIMIT).join('\n') + '...'
      : content;

  return (
    <div
      className={`inline-block w-full py-2 ${greyed ? 'opacity-50 pointer-events-none' : ''}`}
    >
      <div
        className={cn(
          'border w-full overflow-hidden rounded-sm group',
          USER_MESSAGE_APPEARANCE.border
        )}
      >
        {/* Header with executor/variant */}
        <div
          className={cn(
            'w-full px-2 py-1.5 flex items-center gap-1.5 text-xs font-medium',
            USER_MESSAGE_APPEARANCE.headerBg,
            USER_MESSAGE_APPEARANCE.headerText
          )}
        >
          <span>
            {executor}
            {variant && ` / ${variant}`}
          </span>
          <div className="ml-auto flex items-center gap-1">
            {executionProcessId && canFork && !showRetryEditor && (
              <div className="opacity-0 group-hover:opacity-100 focus-within:opacity-100 transition-opacity duration-150">
                <Button
                  onClick={startRetry}
                  variant="ghost"
                  className="p-1 h-auto"
                  disabled={disabled}
                  title={editTitle}
                  aria-label={t('conversation.userMessage.editMessage')}
                  aria-disabled={disabled}
                >
                  <Pencil className="w-3 h-3" />
                </Button>
              </div>
            )}
          </div>
        </div>

        {/* Content area */}
        <div
          className={cn('px-3 py-2 text-sm', USER_MESSAGE_APPEARANCE.contentBg)}
        >
          {showRetryEditor ? (
            <RetryEditorInline
              attempt={taskAttempt as TaskAttempt}
              executionProcessId={executionProcessId as string}
              initialVariant={null}
              onCancelled={() => {
                setIsEditing(false);
              }}
            />
          ) : (
            <>
              <MarkdownRenderer
                content={displayContent}
                className="whitespace-pre-wrap break-words flex flex-col gap-1 font-light"
              />
              {needsCollapse && (
                // Full-width chevron button at bottom to expand/collapse long messages
                <button
                  onClick={(e) => {
                    e.preventDefault();
                    toggle();
                  }}
                  className={cn(
                    'w-full flex justify-center pt-2 -mb-1',
                    'hover:bg-green-100 dark:hover:bg-green-900/30 rounded-b'
                  )}
                  aria-label={
                    expanded
                      ? t('conversation.userMessage.collapseMessage')
                      : t('conversation.userMessage.expandMessage')
                  }
                >
                  <ChevronDown
                    className={cn(
                      'w-5 h-5 transition-transform',
                      USER_MESSAGE_APPEARANCE.headerText,
                      expanded && 'rotate-180'
                    )}
                  />
                </button>
              )}
            </>
          )}
        </div>
      </div>
    </div>
  );
};

export default UserMessage;
