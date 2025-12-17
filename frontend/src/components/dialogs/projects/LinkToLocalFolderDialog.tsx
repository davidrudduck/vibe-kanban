import { useState, useEffect } from 'react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { Folder, AlertCircle } from 'lucide-react';
import NiceModal, { useModal } from '@ebay/nice-modal-react';
import { useProjectMutations } from '@/hooks/useProjectMutations';
import { FolderPickerDialog } from '@/components/dialogs/shared/FolderPickerDialog';
import { useTranslation } from 'react-i18next';
import { defineModal } from '@/lib/modals';
import type { Project } from 'shared/types';

export type LinkToLocalFolderResult = {
  action: 'linked' | 'canceled';
  project?: Project;
};

export interface LinkToLocalFolderDialogProps {
  remoteProjectId: string;
  projectName: string;
}

const LinkToLocalFolderDialogImpl = NiceModal.create<LinkToLocalFolderDialogProps>(
  ({ remoteProjectId, projectName }) => {
    const modal = useModal();
    const { t } = useTranslation('projects');
    const { t: tCommon } = useTranslation('common');

    const [localFolderPath, setLocalFolderPath] = useState<string>('');
    const [customProjectName, setCustomProjectName] = useState<string>('');
    const [error, setError] = useState<string | null>(null);

    const { linkLocalFolder } = useProjectMutations({
      onLinkLocalFolderSuccess: (project) => {
        modal.resolve({
          action: 'linked',
          project,
        } as LinkToLocalFolderResult);
        modal.hide();
      },
      onLinkLocalFolderError: (err) => {
        setError(
          err instanceof Error
            ? err.message
            : t('linkToLocalFolderDialog.errors.linkFailed')
        );
      },
    });

    const isSubmitting = linkLocalFolder.isPending;

    useEffect(() => {
      if (modal.visible) {
        // Reset form when dialog opens
        setLocalFolderPath('');
        setCustomProjectName(projectName);
        setError(null);
      }
    }, [modal.visible, projectName]);

    const handleBrowse = async () => {
      const selectedPath = await FolderPickerDialog.show({
        title: t('linkToLocalFolderDialog.selectFolder'),
        description: t('linkToLocalFolderDialog.selectFolderDescription'),
        value: localFolderPath,
      });
      if (selectedPath) {
        setLocalFolderPath(selectedPath);
        // Auto-fill project name from folder name if not already set
        if (!customProjectName) {
          const folderName = selectedPath.split('/').filter(Boolean).pop();
          if (folderName) {
            setCustomProjectName(folderName);
          }
        }
        setError(null);
      }
    };

    const handleLink = () => {
      if (!localFolderPath.trim()) {
        setError(t('linkToLocalFolderDialog.errors.selectFolder'));
        return;
      }

      setError(null);

      linkLocalFolder.mutate({
        remote_project_id: remoteProjectId,
        local_folder_path: localFolderPath.trim(),
        project_name: customProjectName.trim() || null,
      });
    };

    const handleCancel = () => {
      modal.resolve({ action: 'canceled' } as LinkToLocalFolderResult);
      modal.hide();
    };

    const handleOpenChange = (open: boolean) => {
      if (!open) {
        handleCancel();
      }
    };

    const canSubmit = () => {
      return !isSubmitting && localFolderPath.trim().length > 0;
    };

    return (
      <Dialog open={modal.visible} onOpenChange={handleOpenChange}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>{t('linkToLocalFolderDialog.title')}</DialogTitle>
            <DialogDescription>
              {t('linkToLocalFolderDialog.description')}
            </DialogDescription>
          </DialogHeader>

          <div className="space-y-4">
            <div className="space-y-2">
              <Label>{t('linkToLocalFolderDialog.remoteProjectLabel')}</Label>
              <div className="px-3 py-2 bg-muted rounded-md text-sm">
                {projectName}
              </div>
            </div>

            <div className="space-y-2">
              <Label htmlFor="local-folder-path">
                {t('linkToLocalFolderDialog.localFolderLabel')}{' '}
                <span className="text-destructive">*</span>
              </Label>
              <div className="flex space-x-2">
                <Input
                  id="local-folder-path"
                  type="text"
                  value={localFolderPath}
                  onChange={(e) => {
                    setLocalFolderPath(e.target.value);
                    setError(null);
                  }}
                  placeholder={t('linkToLocalFolderDialog.localFolderPlaceholder')}
                  disabled={isSubmitting}
                  className="flex-1"
                />
                <Button
                  type="button"
                  variant="outline"
                  onClick={handleBrowse}
                  disabled={isSubmitting}
                >
                  <Folder className="h-4 w-4" />
                </Button>
              </div>
              <p className="text-xs text-muted-foreground">
                {t('linkToLocalFolderDialog.localFolderHelp')}
              </p>
            </div>

            <div className="space-y-2">
              <Label htmlFor="project-name">
                {t('linkToLocalFolderDialog.projectNameLabel')}
              </Label>
              <Input
                id="project-name"
                type="text"
                value={customProjectName}
                onChange={(e) => {
                  setCustomProjectName(e.target.value);
                  setError(null);
                }}
                placeholder={t('linkToLocalFolderDialog.projectNamePlaceholder')}
                disabled={isSubmitting}
              />
              <p className="text-xs text-muted-foreground">
                {t('linkToLocalFolderDialog.projectNameHelp')}
              </p>
            </div>

            {error && (
              <Alert variant="destructive">
                <AlertCircle className="h-4 w-4" />
                <AlertDescription>{error}</AlertDescription>
              </Alert>
            )}
          </div>

          <DialogFooter>
            <Button
              variant="outline"
              onClick={handleCancel}
              disabled={isSubmitting}
            >
              {tCommon('buttons.cancel')}
            </Button>
            <Button onClick={handleLink} disabled={!canSubmit()}>
              {isSubmitting
                ? t('linkToLocalFolderDialog.linking')
                : t('linkToLocalFolderDialog.linkButton')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    );
  }
);

export const LinkToLocalFolderDialog = defineModal<
  LinkToLocalFolderDialogProps,
  LinkToLocalFolderResult
>(LinkToLocalFolderDialogImpl);
