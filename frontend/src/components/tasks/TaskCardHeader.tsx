import type { ReactNode } from 'react';
import { UserAvatar } from './UserAvatar';
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from '@/components/ui/tooltip';
import { Server, User } from 'lucide-react';

interface HeaderAvatar {
  firstName?: string;
  lastName?: string;
  username?: string;
  imageUrl?: string;
  /** Owner name to display in tooltip */
  ownerName?: string | null;
  /** Node name to display in tooltip */
  nodeName?: string | null;
}

interface TaskCardHeaderProps {
  title: ReactNode;
  avatar?: HeaderAvatar;
  right?: ReactNode;
  className?: string;
  titleClassName?: string;
}

export function TaskCardHeader({
  title,
  avatar,
  right,
  className,
  titleClassName,
}: TaskCardHeaderProps) {
  const showTooltip = avatar && (avatar.ownerName || avatar.nodeName);

  const avatarElement = avatar ? (
    <UserAvatar
      firstName={avatar.firstName}
      lastName={avatar.lastName}
      username={avatar.username}
      imageUrl={avatar.imageUrl}
      className="mr-2 inline-flex align-middle h-5 w-5"
    />
  ) : null;

  return (
    <div className={`flex items-start gap-3 min-w-0 ${className ?? ''}`}>
      <h4
        className={`flex-1 min-w-0 line-clamp-2 font-light text-sm ${titleClassName ?? ''}`}
      >
        {showTooltip ? (
          <TooltipProvider>
            <Tooltip>
              <TooltipTrigger asChild>
                <span className="inline-flex align-middle cursor-default">
                  {avatarElement}
                </span>
              </TooltipTrigger>
              <TooltipContent side="top" className="max-w-[200px]">
                <div className="flex flex-col gap-1 text-xs">
                  {avatar.ownerName && (
                    <div className="flex items-center gap-1.5">
                      <User className="h-3 w-3 shrink-0" />
                      <span className="truncate">{avatar.ownerName}</span>
                    </div>
                  )}
                  {avatar.nodeName && (
                    <div className="flex items-center gap-1.5">
                      <Server className="h-3 w-3 shrink-0" />
                      <span className="truncate">{avatar.nodeName}</span>
                    </div>
                  )}
                </div>
              </TooltipContent>
            </Tooltip>
          </TooltipProvider>
        ) : (
          avatarElement
        )}
        <span className="align-middle">{title}</span>
      </h4>
      {right ? (
        <div className="flex items-center gap-1 shrink-0">{right}</div>
      ) : null}
    </div>
  );
}
