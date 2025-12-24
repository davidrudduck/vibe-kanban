import { useEffect, useRef, useCallback, useState } from 'react';
import { Terminal } from '@xterm/xterm';
import { FitAddon } from '@xterm/addon-fit';
import '@xterm/xterm/css/xterm.css';
import {
  useTerminalWebSocket,
  ConnectionState,
} from '@/hooks/useTerminalWebSocket';
import { getTerminalSettings } from '@/hooks/useTerminalSettings';
import { Loader2, RefreshCw, AlertCircle } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { cn } from '@/lib/utils';

interface TerminalViewProps {
  sessionId: string;
  /** Whether this is a reconnection to an existing session */
  isReconnect?: boolean;
  className?: string;
  onClose?: () => void;
}

function TerminalView({
  sessionId,
  isReconnect = false,
  className,
  onClose,
}: TerminalViewProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const terminalRef = useRef<Terminal | null>(null);
  const fitAddonRef = useRef<FitAddon | null>(null);
  const resizeObserverRef = useRef<ResizeObserver | null>(null);
  const [localConnectionState, setLocalConnectionState] =
    useState<ConnectionState>('connecting');
  const [showReconnectBanner, setShowReconnectBanner] = useState(false);

  const handleOutput = useCallback((data: string) => {
    terminalRef.current?.write(data);
  }, []);

  const handleExit = useCallback(
    (code: number | null) => {
      terminalRef.current?.writeln(
        `\r\n\x1b[90m[Session exited with code ${code ?? 'unknown'}]\x1b[0m`
      );
      onClose?.();
    },
    [onClose]
  );

  const handleError = useCallback((message: string) => {
    terminalRef.current?.writeln(`\r\n\x1b[31m[Error: ${message}]\x1b[0m`);
  }, []);

  const handleConnectionChange = useCallback((state: ConnectionState) => {
    setLocalConnectionState(state);
  }, []);

  const { sendInput, sendResize, connectionState, reconnect, retryCount } =
    useTerminalWebSocket({
      sessionId,
      onOutput: handleOutput,
      onExit: handleExit,
      onError: handleError,
      onConnectionChange: handleConnectionChange,
    });

  // Sync local state with WebSocket state
  useEffect(() => {
    setLocalConnectionState(connectionState);
  }, [connectionState]);

  // Initialize terminal
  useEffect(() => {
    if (!containerRef.current) return;

    // Load settings at terminal creation time
    const terminalSettings = getTerminalSettings();

    const terminal = new Terminal({
      cursorBlink: terminalSettings.cursorBlink,
      fontSize: terminalSettings.fontSize,
      fontFamily: terminalSettings.fontFamily,
      scrollSensitivity: terminalSettings.scrollSensitivity,
      theme: {
        background: '#1a1b26',
        foreground: '#a9b1d6',
        cursor: '#c0caf5',
        cursorAccent: '#1a1b26',
        selectionBackground: '#33467c',
        black: '#32344a',
        red: '#f7768e',
        green: '#9ece6a',
        yellow: '#e0af68',
        blue: '#7aa2f7',
        magenta: '#ad8ee6',
        cyan: '#449dab',
        white: '#787c99',
        brightBlack: '#444b6a',
        brightRed: '#ff7a93',
        brightGreen: '#b9f27c',
        brightYellow: '#ff9e64',
        brightBlue: '#7da6ff',
        brightMagenta: '#bb9af7',
        brightCyan: '#0db9d7',
        brightWhite: '#acb0d0',
      },
    });

    const fitAddon = new FitAddon();
    terminal.loadAddon(fitAddon);

    terminal.open(containerRef.current);
    terminalRef.current = terminal;
    fitAddonRef.current = fitAddon;

    // Initial fit
    requestAnimationFrame(() => {
      fitAddon.fit();
      sendResize(terminal.cols, terminal.rows);
    });

    // Handle user input
    const onDataDisposable = terminal.onData((data) => {
      sendInput(data);
    });

    // Handle resize events
    const onResizeDisposable = terminal.onResize(({ cols, rows }) => {
      sendResize(cols, rows);
    });

    // Set up ResizeObserver for container size changes
    const resizeObserver = new ResizeObserver(() => {
      requestAnimationFrame(() => {
        fitAddon.fit();
      });
    });
    resizeObserver.observe(containerRef.current);
    resizeObserverRef.current = resizeObserver;

    return () => {
      onDataDisposable.dispose();
      onResizeDisposable.dispose();
      resizeObserver.disconnect();
      terminal.dispose();
      terminalRef.current = null;
      fitAddonRef.current = null;
    };
  }, [sendInput, sendResize]);

  // Focus terminal when connected
  useEffect(() => {
    if (localConnectionState === 'connected') {
      terminalRef.current?.focus();
    }
  }, [localConnectionState]);

  // Show reconnect banner briefly when reconnecting to existing session
  useEffect(() => {
    if (isReconnect && localConnectionState === 'connected') {
      setShowReconnectBanner(true);
      const timer = setTimeout(() => {
        setShowReconnectBanner(false);
      }, 2000); // Hide after 2 seconds
      return () => clearTimeout(timer);
    }
  }, [isReconnect, localConnectionState]);

  return (
    <div className={cn('flex flex-col h-full w-full relative', className)}>
      {/* Connection status overlay */}
      {localConnectionState !== 'connected' && (
        <div className="absolute inset-0 z-10 flex items-center justify-center bg-background/80 backdrop-blur-sm">
          {localConnectionState === 'connecting' && (
            <div className="flex flex-col items-center gap-2 text-muted-foreground">
              {isReconnect ? (
                <>
                  <RefreshCw className="h-8 w-8 animate-spin" />
                  <span>Reconnecting to session...</span>
                </>
              ) : (
                <>
                  <Loader2 className="h-8 w-8 animate-spin" />
                  <span>
                    {retryCount > 0
                      ? `Retrying connection... (${retryCount}/6)`
                      : 'Connecting...'}
                  </span>
                </>
              )}
            </div>
          )}
          {(localConnectionState === 'error' ||
            localConnectionState === 'disconnected') && (
            <div className="flex flex-col items-center gap-3 text-center px-4 max-w-sm">
              <div className="w-12 h-12 rounded-full bg-destructive/10 flex items-center justify-center">
                <AlertCircle className="h-6 w-6 text-destructive" />
              </div>
              <div className="space-y-1">
                <h3 className="font-medium text-foreground">
                  {localConnectionState === 'error'
                    ? 'Connection Failed'
                    : 'Disconnected'}
                </h3>
                <p className="text-sm text-muted-foreground">
                  {localConnectionState === 'error'
                    ? 'Unable to connect to the terminal session. The session may have expired or the server may be unreachable.'
                    : 'The terminal connection was lost. You can try to reconnect.'}
                </p>
              </div>
              <Button
                variant="default"
                size="sm"
                onClick={reconnect}
                className="gap-2 mt-1"
              >
                <RefreshCw className="h-4 w-4" />
                Try Again
              </Button>
            </div>
          )}
        </div>
      )}

      {/* Reconnection success banner */}
      {showReconnectBanner && (
        <div className="absolute top-2 left-1/2 -translate-x-1/2 z-20 px-3 py-1.5 rounded-md bg-green-600/90 text-white text-sm font-medium flex items-center gap-2 animate-in fade-in slide-in-from-top-2 duration-200">
          <RefreshCw className="h-4 w-4" />
          Session restored
        </div>
      )}

      {/* Terminal container */}
      <div
        ref={containerRef}
        className="flex-1 min-h-0 w-full"
        style={{ padding: '4px' }}
      />
    </div>
  );
}

export default TerminalView;
