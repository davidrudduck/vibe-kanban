import { useRef } from 'react';
import { useTranslation } from 'react-i18next';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from '@/components/ui/card';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { Button } from '@/components/ui/button';
import { Download, Loader2, Plus, Trash2, Upload } from 'lucide-react';
import { backupsApi } from '@/lib/api';
import type { BackupInfo } from 'shared/types';
import { useFeedback } from '@/hooks/useFeedback';

function formatBytes(bytes: bigint): string {
  const numBytes = Number(bytes);
  if (numBytes === 0) return '0 B';
  const k = 1024;
  const sizes = ['B', 'KB', 'MB', 'GB'];
  const i = Math.floor(Math.log(numBytes) / Math.log(k));
  return `${parseFloat((numBytes / Math.pow(k, i)).toFixed(1))} ${sizes[i]}`;
}

function formatDate(date: Date | string): string {
  const d = new Date(date);
  return d.toLocaleString();
}

export function BackupSettings() {
  const { t } = useTranslation('settings');
  const queryClient = useQueryClient();
  const {
    success,
    error: feedbackError,
    showSuccess,
    showError,
  } = useFeedback();
  const fileInputRef = useRef<HTMLInputElement>(null);

  const {
    data: backups,
    isLoading,
    error,
  } = useQuery({
    queryKey: ['backups'],
    queryFn: backupsApi.list,
  });

  const createBackupMutation = useMutation({
    mutationFn: backupsApi.create,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['backups'] });
      showSuccess(t('settings.backups.createSuccess'));
    },
    onError: (err) => {
      console.error('Failed to create backup:', err);
      showError(t('settings.backups.createError'));
    },
  });

  const deleteBackupMutation = useMutation({
    mutationFn: backupsApi.delete,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['backups'] });
      showSuccess(t('settings.backups.deleteSuccess'));
    },
    onError: (err) => {
      console.error('Failed to delete backup:', err);
      showError(t('settings.backups.deleteError'));
    },
  });

  const restoreBackupMutation = useMutation({
    mutationFn: backupsApi.restore,
    onSuccess: (message) => {
      queryClient.invalidateQueries({ queryKey: ['backups'] });
      showSuccess(message);
    },
    onError: (err) => {
      console.error('Failed to restore backup:', err);
      showError(t('settings.backups.restoreError'));
    },
  });

  const handleDeleteBackup = (filename: string) => {
    const confirmed = window.confirm(t('settings.backups.confirmDelete'));
    if (!confirmed) return;
    deleteBackupMutation.mutate(filename);
  };

  const handleUploadClick = () => {
    fileInputRef.current?.click();
  };

  const handleFileChange = (event: React.ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0];
    if (!file) return;

    const confirmed = window.confirm(t('settings.backups.confirmRestore'));
    if (!confirmed) {
      // Reset the file input
      if (fileInputRef.current) {
        fileInputRef.current.value = '';
      }
      return;
    }

    restoreBackupMutation.mutate(file);

    // Reset the file input after upload
    if (fileInputRef.current) {
      fileInputRef.current.value = '';
    }
  };

  if (isLoading) {
    return (
      <div className="flex items-center justify-center py-8">
        <Loader2 className="h-8 w-8 animate-spin" />
        <span className="ml-2">{t('settings.backups.loading')}</span>
      </div>
    );
  }

  if (error) {
    return (
      <div className="text-center py-8 text-destructive">
        {t('settings.backups.loadError')}
      </div>
    );
  }

  return (
    <div className="space-y-6">
      {feedbackError && (
        <Alert variant="destructive">
          <AlertDescription>{feedbackError}</AlertDescription>
        </Alert>
      )}

      {success && (
        <Alert variant="success">
          <AlertDescription className="font-medium">
            {typeof success === 'string'
              ? success
              : t('settings.backups.createSuccess')}
          </AlertDescription>
        </Alert>
      )}

      <Card>
        <CardHeader className="flex flex-row items-start justify-between">
          <div className="space-y-1">
            <CardTitle>{t('settings.backups.title')}</CardTitle>
            <CardDescription>
              {t('settings.backups.description')}
            </CardDescription>
          </div>
          <div className="flex gap-2">
            <input
              ref={fileInputRef}
              type="file"
              accept=".sqlite"
              onChange={handleFileChange}
              className="hidden"
            />
            <Button
              variant="outline"
              onClick={handleUploadClick}
              disabled={restoreBackupMutation.isPending}
            >
              {restoreBackupMutation.isPending ? (
                <Loader2 className="mr-2 h-4 w-4 animate-spin" />
              ) : (
                <Upload className="mr-2 h-4 w-4" />
              )}
              {t('settings.backups.actions.upload')}
            </Button>
            <Button
              onClick={() => createBackupMutation.mutate()}
              disabled={createBackupMutation.isPending}
            >
              {createBackupMutation.isPending ? (
                <Loader2 className="mr-2 h-4 w-4 animate-spin" />
              ) : (
                <Plus className="mr-2 h-4 w-4" />
              )}
              {t('settings.backups.actions.create')}
            </Button>
          </div>
        </CardHeader>
        <CardContent>
          {!backups || backups.length === 0 ? (
            <div className="text-center py-8 text-muted-foreground">
              {t('settings.backups.noBackups')}
            </div>
          ) : (
            <div className="border rounded-lg overflow-hidden">
              <div className="max-h-[400px] overflow-auto">
                <table className="w-full">
                  <thead className="border-b bg-muted/50 sticky top-0">
                    <tr>
                      <th className="text-left p-3 text-sm font-medium">
                        {t('settings.backups.table.filename')}
                      </th>
                      <th className="text-left p-3 text-sm font-medium">
                        {t('settings.backups.table.created')}
                      </th>
                      <th className="text-right p-3 text-sm font-medium">
                        {t('settings.backups.table.size')}
                      </th>
                      <th className="text-right p-3 text-sm font-medium">
                        {t('settings.backups.table.actions')}
                      </th>
                    </tr>
                  </thead>
                  <tbody>
                    {backups.map((backup: BackupInfo) => (
                      <tr
                        key={backup.filename}
                        className="border-b hover:bg-muted/30 transition-colors"
                      >
                        <td className="p-3 text-sm font-mono">
                          {backup.filename}
                        </td>
                        <td className="p-3 text-sm text-muted-foreground">
                          {formatDate(backup.created_at)}
                        </td>
                        <td className="p-3 text-sm text-right text-muted-foreground">
                          {formatBytes(backup.size_bytes)}
                        </td>
                        <td className="p-3 text-right space-x-1">
                          <Button variant="ghost" size="sm" asChild>
                            <a
                              href={backupsApi.getDownloadUrl(backup.filename)}
                              download
                            >
                              <Download className="h-4 w-4" />
                              <span className="sr-only">
                                {t('settings.backups.actions.download')}
                              </span>
                            </a>
                          </Button>
                          <Button
                            variant="ghost"
                            size="sm"
                            onClick={() => handleDeleteBackup(backup.filename)}
                            disabled={deleteBackupMutation.isPending}
                            className="text-destructive hover:text-destructive hover:bg-destructive/10"
                          >
                            <Trash2 className="h-4 w-4" />
                            <span className="sr-only">
                              {t('settings.backups.actions.delete')}
                            </span>
                          </Button>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </div>
          )}
        </CardContent>
      </Card>
    </div>
  );
}
