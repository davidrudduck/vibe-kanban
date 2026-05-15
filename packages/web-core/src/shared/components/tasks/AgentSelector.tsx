import { Bot, ArrowDown } from 'lucide-react';
import { Button } from '@vibe/ui/components/Button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@vibe/ui/components/DropdownMenu';
import { Label } from '@vibe/ui/components/Label';
import type { ExecutorProfileId, BaseCodingAgent } from 'shared/types';
import {
  getExecutorDisplayName,
  getExecutorModeDescription,
} from '@/shared/lib/executor';

interface AgentSelectorProps {
  profiles: Record<string, Record<string, unknown>> | null;
  selectedExecutorProfile: ExecutorProfileId | null;
  onChange: (profile: ExecutorProfileId) => void;
  disabled?: boolean;
  className?: string;
  showLabel?: boolean;
}

export function AgentSelector({
  profiles,
  selectedExecutorProfile,
  onChange,
  disabled,
  className = '',
  showLabel = false,
}: AgentSelectorProps) {
  const agents = profiles
    ? (Object.keys(profiles).sort() as BaseCodingAgent[])
    : [];
  const selectedAgent = selectedExecutorProfile?.executor;
  const selectedDescription = getExecutorModeDescription(selectedAgent);

  if (!profiles) return null;

  return (
    <div className="flex-1">
      {showLabel && (
        <Label htmlFor="executor-profile" className="text-sm font-medium">
          Agent
        </Label>
      )}
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button
            variant="outline"
            size="sm"
            className={`w-full justify-between text-xs ${showLabel ? 'mt-1.5' : ''} ${className}`}
            disabled={disabled}
            aria-label="Select agent"
          >
            <div className="flex items-center gap-1.5 w-full">
              <Bot className="h-3 w-3" />
              <span className="truncate">
                {getExecutorDisplayName(selectedAgent)}
              </span>
            </div>
            <ArrowDown className="h-3 w-3" />
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent className="w-96 max-w-[calc(100vw-2rem)]">
          {agents.length === 0 ? (
            <div className="p-2 text-sm text-muted-foreground text-center">
              No agents available
            </div>
          ) : (
            agents.map((agent) => {
              const description = getExecutorModeDescription(agent);
              return (
                <DropdownMenuItem
                  key={agent}
                  onClick={() => {
                    onChange({
                      executor: agent,
                      variant: null,
                    });
                  }}
                  className={selectedAgent === agent ? 'bg-accent' : ''}
                >
                  <div className="min-w-0">
                    <div className="truncate">
                      {getExecutorDisplayName(agent)}
                    </div>
                    {description && (
                      <div className="whitespace-normal text-xs leading-snug text-muted-foreground">
                        {description}
                      </div>
                    )}
                  </div>
                </DropdownMenuItem>
              );
            })
          )}
        </DropdownMenuContent>
      </DropdownMenu>
      {selectedDescription && (
        <p className="mt-1 text-xs leading-snug text-muted-foreground">
          {selectedDescription}
        </p>
      )}
    </div>
  );
}
