import { useState, useEffect } from 'react';
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
import { Checkbox } from '@/components/ui/checkbox';
import { Label } from '@/components/ui/label';
import { tasksApi } from '@/lib/api';
import type { TaskWithAttemptStatus, Task } from 'shared/types';
import NiceModal, { useModal } from '@ebay/nice-modal-react';
import { defineModal } from '@/lib/modals';

export interface ArchiveTaskConfirmationDialogProps {
  task: TaskWithAttemptStatus;
  projectId: string;
}

const ArchiveTaskConfirmationDialogImpl =
  NiceModal.create<ArchiveTaskConfirmationDialogProps>(({ task }) => {
    const { t } = useTranslation('tasks');
    const modal = useModal();
    const [isArchiving, setIsArchiving] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const [children, setChildren] = useState<Task[]>([]);
    const [loadingChildren, setLoadingChildren] = useState(true);
    const [includeSubtasks, setIncludeSubtasks] = useState(true);

    useEffect(() => {
      const fetchChildren = async () => {
        try {
          const childTasks = await tasksApi.getChildren(task.id);
          setChildren(childTasks);
        } catch (err) {
          console.error('Failed to fetch children:', err);
          // Non-fatal error, just means we won't show subtask option
        } finally {
          setLoadingChildren(false);
        }
      };
      fetchChildren();
    }, [task.id]);

    const handleConfirmArchive = async () => {
      setIsArchiving(true);
      setError(null);

      try {
        await tasksApi.archive(task.id, { include_subtasks: includeSubtasks });
        modal.resolve();
        modal.hide();
      } catch (err: unknown) {
        const errorMessage =
          err instanceof Error ? err.message : t('archiveDialog.genericError');
        setError(errorMessage);
      } finally {
        setIsArchiving(false);
      }
    };

    const handleCancelArchive = () => {
      modal.reject();
      modal.hide();
    };

    const hasChildren = children.length > 0;

    return (
      <Dialog
        open={modal.visible}
        onOpenChange={(open) => !open && handleCancelArchive()}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{t('archiveDialog.title')}</DialogTitle>
            <DialogDescription>
              {t('archiveDialog.description', { title: task.title })}
            </DialogDescription>
          </DialogHeader>

          <Alert className="mb-4">{t('archiveDialog.info')}</Alert>

          {!loadingChildren && hasChildren && (
            <div className="flex items-center space-x-2 mb-4">
              <Checkbox
                id="include-subtasks"
                checked={includeSubtasks}
                onCheckedChange={(checked) => setIncludeSubtasks(checked)}
              />
              <Label htmlFor="include-subtasks" className="cursor-pointer">
                {t('archiveDialog.includeSubtasks', { count: children.length })}
              </Label>
            </div>
          )}

          {error && (
            <Alert variant="destructive" className="mb-4">
              {error}
            </Alert>
          )}

          <DialogFooter>
            <Button
              variant="outline"
              onClick={handleCancelArchive}
              disabled={isArchiving}
              autoFocus
            >
              {t('common:buttons.cancel')}
            </Button>
            <Button
              variant="default"
              onClick={handleConfirmArchive}
              disabled={isArchiving || loadingChildren}
            >
              {isArchiving
                ? t('archiveDialog.archiving')
                : t('archiveDialog.confirm')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    );
  });

export const ArchiveTaskConfirmationDialog = defineModal<
  ArchiveTaskConfirmationDialogProps,
  void
>(ArchiveTaskConfirmationDialogImpl);
