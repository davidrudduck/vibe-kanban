import { useEffect, useRef } from 'react';
import { useWorkspaceContext } from '@/shared/hooks/useWorkspaceContext';
import { useTerminal } from '@/shared/hooks/useTerminal';
import { useExecutionProcessesContext } from '@/shared/hooks/useExecutionProcessesContext';
import { TerminalPanel } from '@vibe/ui/components/TerminalPanel';
import { XTermInstance } from './XTermInstance';
import {
  BaseCodingAgent,
  ExecutionProcess,
  ExecutionProcessStatus,
} from 'shared/types';

function getBaseExecutor(process: ExecutionProcess): BaseCodingAgent | null {
  const action = process.executor_action.typ;
  switch (action.type) {
    case 'CodingAgentInitialRequest':
    case 'CodingAgentFollowUpRequest':
    case 'ReviewRequest':
      return action.executor_config.executor;
    default:
      return null;
  }
}

export function TerminalPanelContainer() {
  const { workspace } = useWorkspaceContext();
  const { executionProcessesVisible } = useExecutionProcessesContext();
  const {
    getTabsForWorkspace,
    getActiveTab,
    createTab,
    closeTab,
    clearWorkspaceTabs,
  } = useTerminal();

  const workspaceId = workspace?.id;
  const containerRef = workspace?.container_ref ?? null;
  const tabs = workspaceId ? getTabsForWorkspace(workspaceId) : [];
  const activeTab = workspaceId ? getActiveTab(workspaceId) : null;
  const runningClaudeTerminalProcess = executionProcessesVisible.find(
    (process) =>
      process.status === ExecutionProcessStatus.running &&
      getBaseExecutor(process) === BaseCodingAgent.CLAUDE_TERMINAL
  );
  const runningClaudeTmuxSession = runningClaudeTerminalProcess
    ? `vk-claude-${runningClaudeTerminalProcess.id}`
    : null;

  const creatingRef = useRef(false);
  const prevWorkspaceIdRef = useRef<string | null>(null);

  // Clean up terminals when workspace changes
  useEffect(() => {
    if (
      prevWorkspaceIdRef.current &&
      prevWorkspaceIdRef.current !== workspaceId
    ) {
      clearWorkspaceTabs(prevWorkspaceIdRef.current);
    }
    prevWorkspaceIdRef.current = workspaceId ?? null;
  }, [workspaceId, clearWorkspaceTabs]);

  // Auto-create a Claude terminal attach tab for running Claude Terminal work.
  useEffect(() => {
    if (!workspaceId || !containerRef || !runningClaudeTmuxSession) {
      return;
    }
    const hasClaudeTab = tabs.some(
      (tab) => tab.tmuxSession === runningClaudeTmuxSession
    );
    if (!hasClaudeTab) {
      createTab(workspaceId, containerRef, {
        title: 'Claude Terminal',
        tmuxSession: runningClaudeTmuxSession,
      });
    }
  }, [workspaceId, containerRef, runningClaudeTmuxSession, tabs, createTab]);

  // Auto-create first shell tab when workspace is selected and no tmux tab is active.
  useEffect(() => {
    if (
      workspaceId &&
      containerRef &&
      tabs.length === 0 &&
      !runningClaudeTmuxSession &&
      !creatingRef.current
    ) {
      creatingRef.current = true;
      createTab(workspaceId, containerRef);
    }
    if (tabs.length > 0) {
      creatingRef.current = false;
    }
  }, [
    workspaceId,
    containerRef,
    tabs.length,
    runningClaudeTmuxSession,
    createTab,
  ]);

  return (
    <TerminalPanel
      tabs={tabs}
      activeTabId={activeTab?.id ?? null}
      renderTab={(tabId, isActive) => (
        <XTermInstance
          key={tabId}
          tabId={tabId}
          workspaceId={workspaceId ?? ''}
          tmuxSession={tabs.find((tab) => tab.id === tabId)?.tmuxSession}
          isActive={isActive}
          onClose={() => workspaceId && closeTab(workspaceId, tabId)}
        />
      )}
    />
  );
}
