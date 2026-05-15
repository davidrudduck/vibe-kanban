import { useState } from 'react';
import { StopIcon, TerminalIcon } from '@phosphor-icons/react';

import { XTermInstance } from '@/shared/components/XTermInstance';
import { executionProcessesApi } from '@/shared/lib/api';
import type { ExecutionProcess } from 'shared/types';

interface ClaudeTerminalInlinePanelProps {
  process: ExecutionProcess;
  workspaceId: string;
}

export function ClaudeTerminalInlinePanel({
  process,
  workspaceId,
}: ClaudeTerminalInlinePanelProps) {
  const [isStopping, setIsStopping] = useState(false);
  const tabId = `claude-terminal-inline-${process.id}`;
  const tmuxSession = `vk-claude-${process.id}`;

  const handleStop = async () => {
    if (isStopping) return;
    setIsStopping(true);
    try {
      await executionProcessesApi.stopExecutionProcess(process.id);
    } catch (error) {
      console.error('Failed to stop Claude Terminal', error);
    } finally {
      setIsStopping(false);
    }
  };

  return (
    <div className="mx-double my-base overflow-hidden rounded-md border border-border bg-background">
      <div className="flex h-10 items-center justify-between border-b border-border px-base">
        <div className="flex min-w-0 items-center gap-2 text-sm font-medium text-foreground">
          <TerminalIcon className="size-icon-sm shrink-0 text-low" />
          <span className="truncate">Claude Terminal</span>
        </div>
        <button
          type="button"
          className="inline-flex h-7 items-center gap-1.5 rounded-md border border-border px-2 text-xs font-medium text-foreground hover:bg-muted disabled:cursor-not-allowed disabled:opacity-60"
          onClick={handleStop}
          disabled={isStopping}
          title="Stop Claude Terminal"
          aria-label="Stop Claude Terminal"
        >
          <StopIcon className="size-icon-sm" weight="fill" />
          <span>{isStopping ? 'Stopping' : 'Stop'}</span>
        </button>
      </div>
      <div className="h-[420px] min-h-80 bg-black">
        <XTermInstance
          tabId={tabId}
          workspaceId={workspaceId}
          tmuxSession={tmuxSession}
          isActive={false}
        />
      </div>
    </div>
  );
}
