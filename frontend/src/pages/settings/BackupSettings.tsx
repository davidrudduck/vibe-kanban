import { useTranslation } from 'react-i18next';
import { useQuery } from '@tanstack/react-query';
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from '@/components/ui/card';
import { Loader2 } from 'lucide-react';
import { backupsApi } from '@/lib/api';
import type { BackupInfo } from 'shared/types';

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

  const {
    data: backups,
    isLoading,
    error,
  } = useQuery({
    queryKey: ['backups'],
    queryFn: backupsApi.list,
  });

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
      <Card>
        <CardHeader>
          <CardTitle>{t('settings.backups.title')}</CardTitle>
          <CardDescription>{t('settings.backups.description')}</CardDescription>
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
