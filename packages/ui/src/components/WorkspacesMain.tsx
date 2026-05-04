import type { ReactNode, RefObject } from 'react';
import { useTranslation } from 'react-i18next';
import { SpinnerIcon } from '@phosphor-icons/react';
import { cn } from '../lib/cn';
import { ConversationNavOverlay } from './ConversationNavOverlay';

export interface WorkspacesMainWorkspace {
  id: string;
}

interface WorkspacesMainProps {
  workspaceWithSession: WorkspacesMainWorkspace | undefined;
  isLoading: boolean;
  showLoadingOverlay?: boolean;
  containerRef: RefObject<HTMLElement>;
  conversationContent?: ReactNode;
  chatBoxContent: ReactNode;
  contextBarContent?: ReactNode;
  isAtBottom?: boolean;
  isAtTop?: boolean;
  hasPreviousUserMessage?: boolean;
  hasNextUserMessage?: boolean;
  onScrollToBottom?: () => void;
  onScrollToTop?: () => void;
  onScrollToPreviousMessage?: () => void;
  onScrollToNextMessage?: () => void;
  isMobile?: boolean;
}

export function WorkspacesMain({
  workspaceWithSession,
  isLoading,
  showLoadingOverlay = false,
  containerRef,
  conversationContent,
  chatBoxContent,
  contextBarContent,
  isAtBottom = false,
  isAtTop = false,
  hasPreviousUserMessage = false,
  hasNextUserMessage = false,
  onScrollToBottom,
  onScrollToTop,
  onScrollToPreviousMessage,
  onScrollToNextMessage,
  isMobile,
}: WorkspacesMainProps) {
  const { t } = useTranslation(['tasks', 'common']);

  // Always render the main structure to prevent chat box flash during workspace transitions
  return (
    <main
      ref={containerRef}
      className={cn(
        'relative flex flex-1 flex-col bg-primary',
        isMobile ? 'min-h-0' : 'h-full'
      )}
    >
      {/* Conversation content - conditional based on loading/workspace state */}
      {isLoading ? (
        <div className="flex-1 flex items-center justify-center">
          <SpinnerIcon className="size-6 animate-spin text-low" />
        </div>
      ) : !workspaceWithSession ? (
        <div className="flex-1 flex items-center justify-center">
          <p className="text-low">{t('common:workspaces.selectToStart')}</p>
        </div>
      ) : (
        <>
          {showLoadingOverlay && (
            <div className="absolute inset-0 z-10 flex items-center justify-center bg-primary">
              <SpinnerIcon className="size-6 animate-spin text-low" />
            </div>
          )}
          {conversationContent}
        </>
      )}
      {/* Conversation navigation overlay (top, prev user msg, next user msg, bottom) */}
      {workspaceWithSession &&
        onScrollToTop &&
        onScrollToBottom &&
        onScrollToPreviousMessage &&
        onScrollToNextMessage && (
          <ConversationNavOverlay
            isAtTop={isAtTop}
            isAtBottom={isAtBottom}
            hasPreviousUserMessage={hasPreviousUserMessage}
            hasNextUserMessage={hasNextUserMessage}
            onScrollToTop={onScrollToTop}
            onScrollToPreviousMessage={onScrollToPreviousMessage}
            onScrollToNextMessage={onScrollToNextMessage}
            onScrollToBottom={onScrollToBottom}
            isMobile={isMobile}
          />
        )}
      {/* Chat box - always rendered to prevent flash during workspace switch */}
      <div
        className="flex justify-center @container pl-px"
        data-chatbox-container="true"
      >
        {chatBoxContent}
      </div>
      {/* Context Bar - floating toolbar */}
      {workspaceWithSession ? contextBarContent : null}
    </main>
  );
}
