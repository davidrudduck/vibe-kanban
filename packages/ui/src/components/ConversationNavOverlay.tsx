import { useTranslation } from 'react-i18next';
import {
  ArrowDownIcon,
  ArrowLineDownIcon,
  ArrowLineUpIcon,
  ArrowUpIcon,
  type Icon as PhosphorIcon,
} from '@phosphor-icons/react';
import { cn } from '../lib/cn';

export interface ConversationNavOverlayProps {
  isAtTop: boolean;
  isAtBottom: boolean;
  hasPreviousUserMessage: boolean;
  hasNextUserMessage: boolean;
  onScrollToTop: () => void;
  onScrollToPreviousMessage: () => void;
  onScrollToNextMessage: () => void;
  onScrollToBottom: () => void;
  /**
   * On narrow viewports the vertical 4-button stack is omitted; the parent
   * shell is expected to provide its own affordance (or none).
   */
  isMobile?: boolean;
  className?: string;
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

export function ConversationNavOverlay({
  isAtTop,
  isAtBottom,
  hasPreviousUserMessage,
  hasNextUserMessage,
  onScrollToTop,
  onScrollToPreviousMessage,
  onScrollToNextMessage,
  onScrollToBottom,
  isMobile,
  className,
}: ConversationNavOverlayProps) {
  const { t } = useTranslation('common');

  if (isMobile) return null;
  if (isAtTop && isAtBottom) return null;

  return (
    <div className={cn('flex justify-center pointer-events-none', className)}>
      <div className="w-chat max-w-full relative">
        <div className="absolute bottom-2 right-4 z-10 flex flex-col gap-1 pointer-events-none">
          {!isAtTop && (
            <NavButton
              icon={ArrowLineUpIcon}
              label={t('workspaces.nav.goToTop')}
              onClick={onScrollToTop}
            />
          )}
          {!isAtTop && hasPreviousUserMessage && (
            <NavButton
              icon={ArrowUpIcon}
              label={t('workspaces.nav.previousUserMessage')}
              onClick={onScrollToPreviousMessage}
            />
          )}
          {!isAtBottom && hasNextUserMessage && (
            <NavButton
              icon={ArrowDownIcon}
              label={t('workspaces.nav.nextUserMessage')}
              onClick={onScrollToNextMessage}
            />
          )}
          {!isAtBottom && (
            <NavButton
              icon={ArrowLineDownIcon}
              label={t('workspaces.nav.scrollToBottom')}
              onClick={onScrollToBottom}
            />
          )}
        </div>
      </div>
    </div>
  );
}
