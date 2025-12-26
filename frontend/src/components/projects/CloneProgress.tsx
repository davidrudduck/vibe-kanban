import { Loader2 } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { Card, CardContent } from '@/components/ui/card';
import { Progress } from '@/components/ui/progress';

interface CloneProgressProps {
  cloneUrl: string;
}

export function CloneProgress({ cloneUrl }: CloneProgressProps) {
  const { t } = useTranslation('projects');

  return (
    <Card>
      <CardContent className="p-6">
        <div className="flex flex-col items-center gap-4">
          <Loader2 className="h-8 w-8 animate-spin text-primary" />
          <div className="text-center w-full">
            <div className="font-medium text-foreground">
              {t('createDialog.cloneFromUrl.cloning')}
            </div>
            <div className="text-sm text-muted-foreground mt-1 truncate max-w-md mx-auto">
              {cloneUrl}
            </div>
            <div className="w-full mt-4 max-w-xs mx-auto">
              <Progress indeterminate className="h-2" />
            </div>
            <div className="text-xs text-muted-foreground mt-2">
              {t('createDialog.cloneFromUrl.cloningDescription')}
            </div>
          </div>
        </div>
      </CardContent>
    </Card>
  );
}

export default CloneProgress;
