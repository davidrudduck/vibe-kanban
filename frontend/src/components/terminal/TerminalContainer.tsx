import { useState, useEffect, useCallback, useRef } from 'react';
import { useTerminalSessionMutations } from '@/hooks/useTerminalSession';
import TerminalView from './TerminalView';
import { Loader2, AlertCircle } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { cn } from '@/lib/utils';
import type { CreateSessionResponse } from 'shared/types';

interface TerminalContainerProps {
  workingDir: string;
  className?: string;
  onClose?: () => void;
}

function TerminalContainer({
  workingDir,
  className,
  onClose,
}: TerminalContainerProps) {
  const [sessionId, setSessionId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [isCreating, setIsCreating] = useState(false);
  const [isReconnect, setIsReconnect] = useState(false);

  const handleCreateSuccess = useCallback((response: CreateSessionResponse) => {
    setSessionId(response.session_id);
    setIsReconnect(response.is_reconnect);
    setError(null);
    setIsCreating(false);
  }, []);

  const handleCreateError = useCallback((err: unknown) => {
    const message =
      err instanceof Error ? err.message : 'Failed to create terminal session';
    setError(message);
    setIsCreating(false);
  }, []);

  const { createSession } = useTerminalSessionMutations({
    onCreateSuccess: handleCreateSuccess,
    onCreateError: handleCreateError,
  });

  // Use ref to avoid dependency on createSession which changes on each render
  const createSessionRef = useRef(createSession);
  createSessionRef.current = createSession;

  // Track if a mutation is already in flight to prevent StrictMode double-invocation
  const mutationInFlightRef = useRef(false);

  // Create session on mount - use workingDir as key to detect changes
  useEffect(() => {
    // Prevent double invocation from React StrictMode
    if (mutationInFlightRef.current) {
      return;
    }

    // Reset state when workingDir changes
    setSessionId(null);
    setError(null);
    setIsCreating(true);
    setIsReconnect(false);

    mutationInFlightRef.current = true;
    createSessionRef.current.mutate(workingDir);

    return () => {
      // Allow new mutations after cleanup
      mutationInFlightRef.current = false;
    };
  }, [workingDir]);

  const handleRetry = () => {
    setError(null);
    setSessionId(null);
    setIsCreating(true);
    setIsReconnect(false);
    mutationInFlightRef.current = true;
    createSession.mutate(workingDir);
  };

  if (error) {
    return (
      <div className={cn('flex items-center justify-center h-full', className)}>
        <div className="flex flex-col items-center gap-3 text-muted-foreground">
          <AlertCircle className="h-8 w-8 text-destructive" />
          <span className="text-center max-w-sm">{error}</span>
          <Button variant="outline" size="sm" onClick={handleRetry}>
            Retry
          </Button>
        </div>
      </div>
    );
  }

  if (!sessionId || isCreating) {
    return (
      <div className={cn('flex items-center justify-center h-full', className)}>
        <div className="flex flex-col items-center gap-2 text-muted-foreground">
          <Loader2 className="h-8 w-8 animate-spin" />
          <span>Creating terminal session...</span>
        </div>
      </div>
    );
  }

  return (
    <TerminalView
      sessionId={sessionId}
      isReconnect={isReconnect}
      className={className}
      onClose={onClose}
    />
  );
}

export default TerminalContainer;
