import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import {
  ChevronDown,
  ChevronRight,
  Trash2,
  GripVertical,
  X,
  Loader2,
} from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Textarea } from '@/components/ui/textarea';
import { cn } from '@/lib/utils';
import type { QueuedMessage } from 'shared/types';

type Props = {
  queue: QueuedMessage[];
  isLoading: boolean;
  onUpdate: (messageId: string, content: string | null) => Promise<void>;
  onRemove: (messageId: string) => Promise<void>;
  onReorder: (messageIds: string[]) => Promise<void>;
  onClear: () => Promise<void>;
  isRemoving?: boolean;
  isClearing?: boolean;
};

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
      // Stay in edit mode on error
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
      try {
        await onRemove();
      } catch (error) {
        console.error('Failed to remove message:', error);
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
        >
          <ChevronRight className="h-3 w-3 -rotate-90" />
        </Button>
        <GripVertical className="h-4 w-4 text-muted-foreground" />
        <Button
          variant="ghost"
          size="icon"
          className="h-5 w-5"
          onClick={onMoveDown}
          disabled={index === total - 1}
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
          >
            {message.content}
          </div>
        )}
        {message.variant && (
          <span className="text-xs text-muted-foreground mt-1 block">
            {t('messageQueue.variant')} {message.variant}
          </span>
        )}
      </div>

      <Button
        variant="ghost"
        size="icon"
        className="h-6 w-6 opacity-0 group-hover:opacity-100 transition-opacity"
        onClick={handleRemove}
        disabled={isRemoving}
      >
        <X className="h-4 w-4" />
      </Button>
    </div>
  );
}

function MessageQueuePanel({
  queue,
  isLoading,
  onUpdate,
  onRemove,
  onReorder,
  onClear,
  isRemoving,
  isClearing,
}: Props) {
  const { t } = useTranslation('tasks');
  const [isExpanded, setIsExpanded] = useState(true);

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

  if (isLoading) {
    return (
      <div className="p-3 text-sm text-muted-foreground">
        {t('messageQueue.loadingQueue')}
      </div>
    );
  }

  return (
    <div className="border rounded-lg overflow-hidden">
      <button
        className={cn(
          'w-full flex items-center justify-between px-3 py-2 bg-muted/50 hover:bg-muted transition-colors',
          'text-sm font-medium'
        )}
        onClick={() => setIsExpanded(!isExpanded)}
      >
        <div className="flex items-center gap-2">
          {isExpanded ? (
            <ChevronDown className="h-4 w-4" />
          ) : (
            <ChevronRight className="h-4 w-4" />
          )}
          <span>{t('messageQueue.title')}</span>
          {queue.length > 0 && (
            <span className="px-1.5 py-0.5 text-xs rounded-full bg-primary/10 text-primary">
              {queue.length}
            </span>
          )}
        </div>
        {queue.length > 0 && (
          <Button
            variant="ghost"
            size="sm"
            className="h-6 px-2 text-xs"
            onClick={(e) => {
              e.stopPropagation();
              onClear();
            }}
            disabled={isClearing}
          >
            <Trash2 className="h-3 w-3 mr-1" />
            {t('messageQueue.clear')}
          </Button>
        )}
      </button>

      {isExpanded && (
        <div className="p-2 space-y-2">
          {queue.length === 0 ? (
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
      )}
    </div>
  );
}

export default MessageQueuePanel;
