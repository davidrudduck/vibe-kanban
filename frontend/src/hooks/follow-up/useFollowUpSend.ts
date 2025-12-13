import { useCallback, useState } from 'react';
import { attemptsApi } from '@/lib/api';
import { getStatusCallback } from '@/contexts/TaskOptimisticContext';
import type { ImageResponse, CreateFollowUpAttempt } from 'shared/types';

type Args = {
  attemptId?: string;
  taskId?: string;
  projectId?: string;
  message: string;
  conflictMarkdown: string | null;
  reviewMarkdown: string;
  clickedMarkdown?: string;
  selectedVariant: string | null;
  images: ImageResponse[];
  newlyUploadedImageIds: string[];
  clearComments: () => void;
  clearClickedElements?: () => void;
  jumpToLogsTab: () => void;
  onAfterSendCleanup: () => void;
  setMessage: (v: string) => void;
};

export function useFollowUpSend({
  attemptId,
  taskId,
  projectId,
  message,
  conflictMarkdown,
  reviewMarkdown,
  clickedMarkdown,
  selectedVariant,
  images,
  newlyUploadedImageIds,
  clearComments,
  clearClickedElements,
  jumpToLogsTab,
  onAfterSendCleanup,
  setMessage,
}: Args) {
  const [isSendingFollowUp, setIsSendingFollowUp] = useState(false);
  const [followUpError, setFollowUpError] = useState<string | null>(null);

  const onSendFollowUp = useCallback(async () => {
    if (!attemptId) return;
    const extraMessage = message.trim();
    const finalPrompt = [
      conflictMarkdown,
      clickedMarkdown?.trim(),
      reviewMarkdown?.trim(),
      extraMessage,
    ]
      .filter(Boolean)
      .join('\n\n');
    if (!finalPrompt) return;
    try {
      setIsSendingFollowUp(true);
      setFollowUpError(null);
      const image_ids =
        newlyUploadedImageIds.length > 0
          ? newlyUploadedImageIds
          : images.length > 0
            ? images.map((img) => img.id)
            : null;
      const body: CreateFollowUpAttempt = {
        prompt: finalPrompt,
        variant: selectedVariant,
        image_ids,
        retry_process_id: null,
        force_when_dirty: null,
        perform_git_reset: null,
      };
      await attemptsApi.followUp(attemptId, body);

      // Optimistically update task status to inprogress for immediate UI feedback
      // This avoids waiting for the WebSocket broadcast which may be delayed
      if (taskId && projectId) {
        const updateStatus = getStatusCallback(projectId);
        updateStatus?.(taskId, 'inprogress');
      }

      setMessage('');
      clearComments();
      clearClickedElements?.();
      onAfterSendCleanup();
      jumpToLogsTab();
    } catch (error: unknown) {
      const err = error as { message?: string };
      setFollowUpError(
        `Failed to start follow-up execution: ${err.message ?? 'Unknown error'}`
      );
    } finally {
      setIsSendingFollowUp(false);
    }
  }, [
    attemptId,
    taskId,
    projectId,
    message,
    conflictMarkdown,
    reviewMarkdown,
    clickedMarkdown,
    newlyUploadedImageIds,
    images,
    selectedVariant,
    clearComments,
    clearClickedElements,
    jumpToLogsTab,
    onAfterSendCleanup,
    setMessage,
  ]);

  return {
    isSendingFollowUp,
    followUpError,
    setFollowUpError,
    onSendFollowUp,
  } as const;
}
