import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Alert } from '@/components/ui/alert';
import { AlertTriangle } from 'lucide-react';
import NiceModal, { useModal } from '@ebay/nice-modal-react';
import { defineModal } from '@/lib/modals';

export interface CleanupWorktreeConfirmationDialogProps {
  attemptId: string;
  worktreePath?: string;
  onConfirm: () => Promise<void>;
}

const CleanupWorktreeConfirmationDialogImpl =
  NiceModal.create<CleanupWorktreeConfirmationDialogProps>(
    ({ worktreePath, onConfirm }) => {
      const { t } = useTranslation('tasks');
      const modal = useModal();
      const [isDeleting, setIsDeleting] = useState(false);
      const [error, setError] = useState<string | null>(null);

      const handleConfirmDelete = async () => {
        setIsDeleting(true);
        setError(null);

        try {
          await onConfirm();
          modal.resolve();
          modal.hide();
        } catch (err: unknown) {
          const errorMessage =
            err instanceof Error
              ? err.message
              : t('cleanupWorktreeDialog.genericError', 'Failed to delete worktree');
          setError(errorMessage);
        } finally {
          setIsDeleting(false);
        }
      };

      const handleCancel = () => {
        modal.reject();
        modal.hide();
      };

      return (
        <Dialog open={modal.visible} onOpenChange={(open) => !open && handleCancel()}>
          <DialogContent>
            <DialogHeader>
              <DialogTitle className="flex items-center gap-2">
                <AlertTriangle className="h-5 w-5 text-destructive" />
                {t('cleanupWorktreeDialog.title', 'Delete Worktree')}
              </DialogTitle>
              <DialogDescription>
                {t(
                  'cleanupWorktreeDialog.description',
                  'This will permanently delete the worktree files from disk. The attempt record will be preserved, but the code changes will be lost unless they were pushed to the remote repository.'
                )}
              </DialogDescription>
            </DialogHeader>

            {worktreePath && (
              <div className="rounded-md bg-muted p-3 text-sm font-mono break-all">
                {worktreePath}
              </div>
            )}

            <Alert variant="destructive" className="mb-4">
              {t(
                'cleanupWorktreeDialog.warning',
                'This action cannot be undone. Make sure any important changes have been committed and pushed.'
              )}
            </Alert>

            {error && (
              <Alert variant="destructive" className="mb-4">
                {error}
              </Alert>
            )}

            <DialogFooter>
              <Button
                variant="outline"
                onClick={handleCancel}
                disabled={isDeleting}
                autoFocus
              >
                {t('common:buttons.cancel')}
              </Button>
              <Button
                variant="destructive"
                onClick={handleConfirmDelete}
                disabled={isDeleting}
              >
                {isDeleting
                  ? t('cleanupWorktreeDialog.deleting', 'Deleting...')
                  : t('cleanupWorktreeDialog.confirm', 'Delete Worktree')}
              </Button>
            </DialogFooter>
          </DialogContent>
        </Dialog>
      );
    }
  );

export const CleanupWorktreeConfirmationDialog = defineModal<
  CleanupWorktreeConfirmationDialogProps,
  void
>(CleanupWorktreeConfirmationDialogImpl);
