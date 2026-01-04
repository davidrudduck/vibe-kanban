import { useTranslation } from 'react-i18next';
import { useBuildInfo } from '@/hooks/useBuildInfo';
import { Loader2, GitCommit, Calendar, GitBranch, Tag } from 'lucide-react';

interface BuildInfoProps {
  className?: string;
}

export function BuildInfo({ className }: BuildInfoProps) {
  const { t } = useTranslation(['settings', 'common']);
  const { buildInfo, isLoading } = useBuildInfo();

  if (isLoading) {
    return (
      <div className={className}>
        <Loader2 className="h-4 w-4 animate-spin text-muted-foreground" />
      </div>
    );
  }

  if (!buildInfo) {
    return null;
  }

  // Format build timestamp
  const formatTimestamp = (timestamp: string) => {
    if (timestamp === 'unknown') return timestamp;
    try {
      const date = new Date(timestamp);
      return date.toLocaleString(undefined, {
        dateStyle: 'medium',
        timeStyle: 'short',
      });
    } catch {
      return timestamp;
    }
  };

  return (
    <div
      className={`text-xs text-muted-foreground border-t pt-4 mt-6 ${className ?? ''}`}
    >
      <div className="flex flex-wrap gap-x-6 gap-y-2">
        <div className="flex items-center gap-1.5">
          <Tag className="h-3.5 w-3.5" />
          <span>{t('settings.general.buildInfo.version')}:</span>
          <code className="bg-muted px-1 py-0.5 rounded text-xs">
            {buildInfo.version}
          </code>
        </div>
        <div className="flex items-center gap-1.5">
          <GitCommit className="h-3.5 w-3.5" />
          <span>{t('settings.general.buildInfo.commit')}:</span>
          <code className="bg-muted px-1 py-0.5 rounded text-xs font-mono">
            {buildInfo.gitCommit}
          </code>
        </div>
        <div className="flex items-center gap-1.5">
          <GitBranch className="h-3.5 w-3.5" />
          <span>{t('settings.general.buildInfo.branch')}:</span>
          <code className="bg-muted px-1 py-0.5 rounded text-xs">
            {buildInfo.gitBranch}
          </code>
        </div>
        <div className="flex items-center gap-1.5">
          <Calendar className="h-3.5 w-3.5" />
          <span>{t('settings.general.buildInfo.built')}:</span>
          <span>{formatTimestamp(buildInfo.buildTimestamp)}</span>
        </div>
      </div>
    </div>
  );
}
