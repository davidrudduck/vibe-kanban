import { AlertCircle } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { useState } from 'react';
import { attemptsApi } from '@/lib/api';

export type Props = Readonly<{
  attemptId: string;
  onFixed: () => void;
}>;

export function SessionErrorBanner({ attemptId, onFixed }: Props) {
  const [fixing, setFixing] = useState(false);

  const handleFix = async () => {
    setFixing(true);
    try {
      await attemptsApi.fixSessions(attemptId);
      onFixed();
    } catch (error) {
      console.error('Failed to fix sessions:', error);
    } finally {
      setFixing(false);
    }
  };

  return (
    <div
      className="flex flex-col gap-2 rounded-md border border-destructive/40 bg-destructive/10 p-3"
      role="status"
      aria-live="polite"
    >
      <div className="flex items-start gap-2">
        <AlertCircle className="mt-0.5 h-4 w-4 text-destructive" aria-hidden />
        <div className="text-sm leading-relaxed">
          <span className="font-medium">Session Error:</span> The agent session
          is corrupted and cannot be resumed. Click "Fix Sessions" to clear the
          corrupted session data, then retry your follow-up.
        </div>
      </div>
      <div className="flex flex-wrap gap-2">
        <Button
          size="sm"
          variant="destructive"
          onClick={handleFix}
          disabled={fixing}
        >
          {fixing ? 'Fixing...' : 'Fix Sessions'}
        </Button>
      </div>
    </div>
  );
}
