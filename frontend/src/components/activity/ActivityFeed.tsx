import { useState } from 'react';
import { Bell, Eye, EyeOff } from 'lucide-react';
import { Button } from '@/components/ui/button';
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from '@/components/ui/popover';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from '@/components/ui/tooltip';
import { useActivityFeed } from '@/hooks/useActivityFeed';
import { useActivityDismiss } from '@/hooks/useActivityDismiss';
import ActivityFeedList from './ActivityFeedList';
import type { ActivityCategory } from 'shared/types';

function ActivityFeed() {
  const [open, setOpen] = useState(false);
  const [showDismissed, setShowDismissed] = useState(false);
  const { data, isLoading } = useActivityFeed({
    enabled: true,
    includeDismissed: showDismissed,
  });

  const { dismissItem, undismissItem } = useActivityDismiss();

  const counts = data?.counts ?? {
    needs_review: 0,
    in_progress: 0,
    completed: 0,
    dismissed: 0,
  };
  const totalCount = counts.needs_review + counts.in_progress;

  const filterByCategory = (category: ActivityCategory | 'all') => {
    if (!data?.items) return [];
    if (category === 'all') return data.items;
    return data.items.filter((item) => item.category === category);
  };

  const handleNavigate = () => {
    setOpen(false);
  };

  const handleDismiss = (taskId: string) => {
    dismissItem.mutate(taskId);
  };

  const handleUndismiss = (taskId: string) => {
    undismissItem.mutate(taskId);
  };

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <Button
          variant="ghost"
          size="icon"
          className="h-9 w-9 relative"
          aria-label="Activity feed"
        >
          <Bell className="h-4 w-4" />
          {totalCount > 0 && (
            <span className="absolute -top-0.5 -right-0.5 flex h-4 w-4 items-center justify-center rounded-full bg-destructive text-[10px] font-medium text-destructive-foreground">
              {totalCount > 9 ? '9+' : totalCount}
            </span>
          )}
        </Button>
      </PopoverTrigger>
      <PopoverContent className="w-80 p-0" align="end" sideOffset={8}>
        <div className="border-b px-3 py-2 flex items-center justify-between">
          <h4 className="font-medium text-sm">Activity</h4>
          {counts.dismissed > 0 && (
            <TooltipProvider>
              <Tooltip>
                <TooltipTrigger asChild>
                  <Button
                    variant={showDismissed ? 'secondary' : 'ghost'}
                    size="icon"
                    onClick={() => setShowDismissed(!showDismissed)}
                    aria-label={
                      showDismissed
                        ? 'Hide dismissed items'
                        : 'Show dismissed items'
                    }
                    className="h-6 w-6"
                  >
                    {showDismissed ? (
                      <EyeOff className="h-3.5 w-3.5" />
                    ) : (
                      <Eye className="h-3.5 w-3.5" />
                    )}
                  </Button>
                </TooltipTrigger>
                <TooltipContent side="bottom">
                  <p>
                    {showDismissed ? 'Hide' : 'Show'} {counts.dismissed}{' '}
                    dismissed
                  </p>
                </TooltipContent>
              </Tooltip>
            </TooltipProvider>
          )}
        </div>

        {isLoading ? (
          <div className="px-3 py-6 text-center text-muted-foreground text-sm">
            Loading...
          </div>
        ) : (
          <Tabs defaultValue="all" className="w-full">
            <TabsList className="w-full justify-start gap-0 rounded-none border-b bg-transparent p-0">
              <TabsTrigger
                value="all"
                className="rounded-none border-b-2 border-transparent px-3 py-1.5 text-xs data-[state=active]:border-primary data-[state=active]:bg-transparent"
              >
                All
              </TabsTrigger>
              <TabsTrigger
                value="needs_review"
                className="rounded-none border-b-2 border-transparent px-3 py-1.5 text-xs data-[state=active]:border-primary data-[state=active]:bg-transparent"
              >
                Review ({counts.needs_review})
              </TabsTrigger>
              <TabsTrigger
                value="in_progress"
                className="rounded-none border-b-2 border-transparent px-3 py-1.5 text-xs data-[state=active]:border-primary data-[state=active]:bg-transparent"
              >
                Running ({counts.in_progress})
              </TabsTrigger>
              <TabsTrigger
                value="completed"
                className="rounded-none border-b-2 border-transparent px-3 py-1.5 text-xs data-[state=active]:border-primary data-[state=active]:bg-transparent"
              >
                Done ({counts.completed})
              </TabsTrigger>
            </TabsList>

            <TabsContent value="all" className="m-0">
              <ActivityFeedList
                items={filterByCategory('all')}
                onNavigate={handleNavigate}
                onDismiss={handleDismiss}
                onUndismiss={handleUndismiss}
              />
            </TabsContent>
            <TabsContent value="needs_review" className="m-0">
              <ActivityFeedList
                items={filterByCategory('needs_review')}
                onNavigate={handleNavigate}
                onDismiss={handleDismiss}
                onUndismiss={handleUndismiss}
              />
            </TabsContent>
            <TabsContent value="in_progress" className="m-0">
              <ActivityFeedList
                items={filterByCategory('in_progress')}
                onNavigate={handleNavigate}
                onDismiss={handleDismiss}
                onUndismiss={handleUndismiss}
              />
            </TabsContent>
            <TabsContent value="completed" className="m-0">
              <ActivityFeedList
                items={filterByCategory('completed')}
                onNavigate={handleNavigate}
                onDismiss={handleDismiss}
                onUndismiss={handleUndismiss}
              />
            </TabsContent>
          </Tabs>
        )}
      </PopoverContent>
    </Popover>
  );
}

export default ActivityFeed;
