import type { ReactNode, RefObject } from 'react';
import { useTranslation } from 'react-i18next';
import {
  ArrowDownIcon,
  ArrowLineDownIcon,
  ArrowLineUpIcon,
  ArrowUpIcon,
  SpinnerIcon,
  type Icon as PhosphorIcon,
} from '@phosphor-icons/react';
import { cn } from '../lib/cn';

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
  onAtBottomChange?: (atBottom: boolean) => void;
  onScrollToBottom?: (behavior?: 'auto' | 'smooth') => void;
  onScrollToTop?: (behavior?: 'auto' | 'smooth') => void;
  onScrollToPreviousMessage?: () => void;
  onScrollToNextMessage?: () => void;
  isMobile?: boolean;
}

interface NavButtonProps {
  icon: PhosphorIcon;
  label: string;
  onClick: () => void;
}

function NavButton({ icon: Icon, label, onClick }: NavButtonProps) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="pointer-events-auto flex items-center justify-center size-8 rounded-full bg-secondary/80 backdrop-blur-sm border border-secondary text-low hover:text-normal hover:bg-secondary shadow-md transition-all"
      aria-label={label}
      title={label}
    >
      <Icon className="size-icon-base" weight="bold" />
    </button>
  );
}

export function WorkspacesMain({
  workspaceWithSession,
  isLoading,
  showLoadingOverlay = false,
  containerRef,
  conversationContent,
  chatBoxContent,
  contextBarContent,
  isAtBottom = true,
  isAtTop = true,
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
      {workspaceWithSession && (!isAtTop || !isAtBottom) && (
        <div className="flex justify-center pointer-events-none">
          <div className="w-chat max-w-full relative">
            <div className="absolute bottom-2 right-4 z-10 flex flex-col gap-1 pointer-events-none">
              {!isAtTop && (
                <NavButton
                  icon={ArrowLineUpIcon}
                  label="Go to top"
                  onClick={() => onScrollToTop?.('auto')}
                />
              )}
              {!isAtTop && onScrollToPreviousMessage && (
                <NavButton
                  icon={ArrowUpIcon}
                  label="Previous user message"
                  onClick={onScrollToPreviousMessage}
                />
              )}
              {!isAtBottom && onScrollToNextMessage && (
                <NavButton
                  icon={ArrowDownIcon}
                  label="Next user message"
                  onClick={onScrollToNextMessage}
                />
              )}
              {!isAtBottom && (
                <NavButton
                  icon={ArrowLineDownIcon}
                  label="Scroll to bottom"
                  onClick={() => onScrollToBottom?.('auto')}
                />
              )}
            </div>
          </div>
        </div>
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
