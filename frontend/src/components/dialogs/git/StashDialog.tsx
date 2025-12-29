import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import NiceModal, { useModal } from '@ebay/nice-modal-react';
import { AlertTriangle, FileText, Loader2 } from 'lucide-react';
import { defineModal } from '@/lib/modals';
import { useStash } from '@/hooks/useStash';
import { useState } from 'react';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { useTranslation } from 'react-i18next';

export interface StashDialogProps {
  attemptId: string;
  dirtyFiles: string[];
  onStashComplete?: () => Promise<void>;
}

export type StashDialogResult = 'stashed' | 'canceled';

const StashDialogImpl = NiceModal.create<StashDialogProps>((props) => {
  const modal = useModal();
  const { attemptId, dirtyFiles, onStashComplete } = props;
  const [error, setError] = useState<string | null>(null);
  const { t } = useTranslation(['tasks', 'common']);

  const stash = useStash(
    attemptId,
    async () => {
      try {
        // After stashing, run the callback (e.g., merge, rebase)
        if (onStashComplete) {
          await onStashComplete();
        }
        modal.resolve('stashed' as StashDialogResult);
        modal.hide();
      } catch (err) {
        // If the operation after stash fails, show error but don't close
        const message =
          err && typeof err === 'object' && 'message' in err
            ? String(err.message)
            : t('tasks:git.stashDialog.popError');
        setError(message);
      }
    },
    (err: unknown) => {
      const message =
        err && typeof err === 'object' && 'message' in err
          ? String(err.message)
          : t('tasks:git.stashDialog.error');
      setError(message);
    }
  );

  const handleStashAndContinue = async () => {
    setError(null);
    try {
      await stash.mutateAsync(undefined);
    } catch {
      // Error already handled by onError callback
    }
  };

  const handleCancel = () => {
    modal.resolve('canceled' as StashDialogResult);
    modal.hide();
  };

  const isProcessing = stash.isPending;

  return (
    <Dialog open={modal.visible} onOpenChange={handleCancel}>
      <DialogContent className="sm:max-w-md lg:max-w-lg">
        <DialogHeader>
          <div className="flex items-center gap-3">
            <AlertTriangle className="h-6 w-6 text-warning shrink-0" />
            <DialogTitle>{t('tasks:git.stashDialog.title')}</DialogTitle>
          </div>
          <DialogDescription className="text-left pt-2">
            {t('tasks:git.stashDialog.description', {
              count: dirtyFiles.length,
            })}
          </DialogDescription>
        </DialogHeader>

        {/* File list */}
        {dirtyFiles.length > 0 && (
          <div className="space-y-2">
            <p className="text-sm font-medium text-muted-foreground">
              {t('tasks:git.stashDialog.filesLabel')}
            </p>
            <div className="max-h-[200px] overflow-y-auto rounded-md border bg-muted/50 p-3">
              <ul className="space-y-1.5">
                {dirtyFiles.map((file) => (
                  <li
                    key={file}
                    className="flex items-center gap-2 text-sm font-mono"
                  >
                    <FileText className="h-4 w-4 text-muted-foreground shrink-0" />
                    <span className="truncate" title={file}>
                      {file}
                    </span>
                  </li>
                ))}
              </ul>
            </div>
          </div>
        )}

        {error && (
          <Alert variant="destructive">
            <AlertDescription>{error}</AlertDescription>
          </Alert>
        )}

        <DialogFooter className="flex-col-reverse gap-2 sm:flex-row sm:justify-end">
          <Button
            variant="outline"
            onClick={handleCancel}
            disabled={isProcessing}
            className="w-full sm:w-auto"
          >
            {t('common:buttons.cancel')}
          </Button>
          <Button
            onClick={handleStashAndContinue}
            disabled={isProcessing}
            className="w-full sm:w-auto"
          >
            {isProcessing && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
            {isProcessing
              ? t('tasks:git.stashDialog.stashing')
              : t('tasks:git.stashDialog.stashAndContinue')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
});

export const StashDialog = defineModal<StashDialogProps, StashDialogResult>(
  StashDialogImpl
);
