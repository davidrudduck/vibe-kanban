import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import {
  MessageSquareDashed,
  Trash2,
  ChevronRight,
  GripVertical,
  X,
  Loader2,
} from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Textarea } from '@/components/ui/textarea';
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from '@/components/ui/popover';
import { cn } from '@/lib/utils';
import type { QueuedMessage } from 'shared/types';

interface MessageQueueBadgeProps {
  queue: QueuedMessage[];
  isLoading: boolean;
  onUpdate: (messageId: string, content: string | null) => Promise<void>;
  onRemove: (messageId: string) => Promise<void>;
  onReorder: (messageIds: string[]) => Promise<void>;
  onClear: () => Promise<void>;
  isRemoving?: boolean;
  isClearing?: boolean;
  className?: string;
}

function MessageQueueItem({
  message,
  index,
  total,
  onUpdate,
  onRemove,
  onMoveUp,
  onMoveDown,
  isRemoving,
  t,
}: {
  message: QueuedMessage;
  index: number;
  total: number;
  onUpdate: (content: string | null) => Promise<void>;
  onRemove: () => Promise<void>;
  onMoveUp: () => void;
  onMoveDown: () => void;
  isRemoving?: boolean;
  t: (key: string) => string;
}) {
  const [isEditing, setIsEditing] = useState(false);
  const [editContent, setEditContent] = useState(message.content);
  const [isSaving, setIsSaving] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);
  const [removeError, setRemoveError] = useState<string | null>(null);

  const handleSave = async () => {
    if (!editContent.trim() || editContent === message.content) {
      setIsEditing(false);
      setSaveError(null);
      return;
    }
    setIsSaving(true);
    setSaveError(null);
    try {
      await onUpdate(editContent.trim());
      setIsEditing(false);
    } catch (error) {
      console.error('Failed to save message:', error);
      setSaveError(t('messageQueue.saveError'));
    } finally {
      setIsSaving(false);
    }
  };

  const handleCancel = () => {
    setEditContent(message.content);
    setIsEditing(false);
    setSaveError(null);
  };

  const handleRemove = async () => {
    if (window.confirm(t('messageQueue.confirmRemove'))) {
      setRemoveError(null);
      try {
        await onRemove();
      } catch (error) {
        console.error('Failed to remove message:', error);
        setRemoveError(t('messageQueue.removeError'));
      }
    }
  };

  return (
    <div className="group flex items-start gap-2 p-2 rounded-md border bg-card hover:bg-accent/50 transition-colors">
      <div className="flex flex-col gap-0.5 pt-1">
        <Button
          variant="ghost"
          size="icon"
          className="h-5 w-5"
          onClick={onMoveUp}
          disabled={index === 0}
          aria-label={t('messageQueue.moveUp')}
        >
          <ChevronRight className="h-3 w-3 -rotate-90" />
        </Button>
        <GripVertical className="h-4 w-4 text-muted-foreground" aria-hidden />
        <Button
          variant="ghost"
          size="icon"
          className="h-5 w-5"
          onClick={onMoveDown}
          disabled={index === total - 1}
          aria-label={t('messageQueue.moveDown')}
        >
          <ChevronRight className="h-3 w-3 rotate-90" />
        </Button>
      </div>

      <div className="flex-1 min-w-0">
        {isEditing ? (
          <div className="space-y-2">
            {saveError && (
              <div className="text-xs text-destructive">{saveError}</div>
            )}
            <Textarea
              value={editContent}
              onChange={(e) => setEditContent(e.target.value)}
              className="min-h-[60px] text-sm"
              autoFocus
            />
            <div className="flex gap-2">
              <Button size="sm" onClick={handleSave} disabled={isSaving}>
                {isSaving ? (
                  <Loader2 className="h-3 w-3 animate-spin" />
                ) : (
                  t('messageQueue.save')
                )}
              </Button>
              <Button size="sm" variant="ghost" onClick={handleCancel}>
                {t('messageQueue.cancel')}
              </Button>
            </div>
          </div>
        ) : (
          <div
            className="text-sm whitespace-pre-wrap break-words cursor-pointer hover:text-foreground/80"
            onClick={() => setIsEditing(true)}
            role="button"
            tabIndex={0}
            onKeyDown={(e) => {
              if (e.key === 'Enter' || e.key === ' ') {
                setIsEditing(true);
              }
            }}
          >
            {message.content}
          </div>
        )}
        {message.variant && (
          <span className="text-xs text-muted-foreground mt-1 block">
            {t('messageQueue.variant')} {message.variant}
          </span>
        )}
        {removeError && (
          <div className="text-xs text-destructive mt-1">{removeError}</div>
        )}
      </div>

      <Button
        variant="ghost"
        size="icon"
        className="h-6 w-6 opacity-0 group-hover:opacity-100 transition-opacity"
        onClick={handleRemove}
        disabled={isRemoving}
        aria-label={t('messageQueue.remove')}
      >
        <X className="h-4 w-4" />
      </Button>
    </div>
  );
}

/**
 * Compact badge showing message queue count with popover for full list.
 * Designed for the toolbar above the input area.
 * Always renders (even with 0 messages) for consistent layout.
 */
export function MessageQueueBadge({
  queue,
  isLoading,
  onUpdate,
  onRemove,
  onReorder,
  onClear,
  isRemoving,
  isClearing,
  className,
}: MessageQueueBadgeProps) {
  const { t } = useTranslation('tasks');

  const handleMoveUp = (index: number) => {
    if (index === 0) return;
    const newOrder = [...queue];
    [newOrder[index - 1], newOrder[index]] = [newOrder[index], newOrder[index - 1]];
    onReorder(newOrder.map((m) => m.id));
  };

  const handleMoveDown = (index: number) => {
    if (index === queue.length - 1) return;
    const newOrder = [...queue];
    [newOrder[index], newOrder[index + 1]] = [newOrder[index + 1], newOrder[index]];
    onReorder(newOrder.map((m) => m.id));
  };

  return (
    <Popover>
      <PopoverTrigger asChild>
        <Button
          variant="ghost"
          size="sm"
          className={cn(
            'h-8 px-2 text-xs font-medium tabular-nums min-h-[44px] min-w-[44px]',
            queue.length > 0 && 'text-amber-600 dark:text-amber-500',
            className
          )}
          aria-label={t('messageQueue.badgeLabel', { count: queue.length })}
        >
          <span className="flex items-center gap-1">
            <MessageSquareDashed className="h-3.5 w-3.5" aria-hidden />
            <span className="hidden sm:inline ml-1">{t('messageQueue.badgeText')}</span>
            <span className="font-mono text-xs">({queue.length})</span>
          </span>
        </Button>
      </PopoverTrigger>
      <PopoverContent
        className="w-72 sm:w-80 lg:w-96 p-0"
        align="start"
        sideOffset={4}
      >
        <div className="flex items-center justify-between px-3 py-2 border-b">
          <h4 className="text-sm font-medium">
            {t('messageQueue.title')} ({queue.length})
          </h4>
          {queue.length > 0 && (
            <Button
              variant="ghost"
              size="sm"
              className="h-6 px-2 text-xs"
              onClick={onClear}
              disabled={isClearing}
            >
              {isClearing ? (
                <Loader2 className="h-3 w-3 animate-spin mr-1" />
              ) : (
                <Trash2 className="h-3 w-3 mr-1" />
              )}
              {t('messageQueue.clear')}
            </Button>
          )}
        </div>
        <div className="max-h-64 overflow-y-auto p-2 space-y-2">
          {isLoading ? (
            <div className="flex items-center justify-center py-4">
              <Loader2 className="h-4 w-4 animate-spin mr-2" />
              <span className="text-sm text-muted-foreground">
                {t('messageQueue.loadingQueue')}
              </span>
            </div>
          ) : queue.length === 0 ? (
            <p className="text-sm text-muted-foreground text-center py-4">
              {t('messageQueue.emptyState')}
            </p>
          ) : (
            queue.map((message, index) => (
              <MessageQueueItem
                key={message.id}
                message={message}
                index={index}
                total={queue.length}
                onUpdate={async (content) => {
                  await onUpdate(message.id, content);
                }}
                onRemove={() => onRemove(message.id)}
                onMoveUp={() => handleMoveUp(index)}
                onMoveDown={() => handleMoveDown(index)}
                isRemoving={isRemoving}
                t={t}
              />
            ))
          )}
        </div>
      </PopoverContent>
    </Popover>
  );
}

export default MessageQueueBadge;
