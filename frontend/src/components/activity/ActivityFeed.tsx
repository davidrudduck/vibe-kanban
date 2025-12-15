import { useState } from 'react';
import { Bell } from 'lucide-react';
import { Button } from '@/components/ui/button';
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from '@/components/ui/popover';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { useActivityFeed } from '@/hooks/useActivityFeed';
import ActivityFeedList from './ActivityFeedList';
import type { ActivityCategory } from 'shared/types';

function ActivityFeed() {
  const [open, setOpen] = useState(false);
  const { data, isLoading } = useActivityFeed({ enabled: true });

  const counts = data?.counts ?? { needs_review: 0, in_progress: 0, completed: 0 };
  const totalCount = counts.needs_review + counts.in_progress;

  const filterByCategory = (category: ActivityCategory | 'all') => {
    if (!data?.items) return [];
    if (category === 'all') return data.items;
    return data.items.filter((item) => item.category === category);
  };

  const handleNavigate = () => {
    setOpen(false);
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
      <PopoverContent
        className="w-80 p-0"
        align="end"
        sideOffset={8}
      >
        <div className="border-b px-3 py-2">
          <h4 className="font-medium text-sm">Activity</h4>
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
              />
            </TabsContent>
            <TabsContent value="needs_review" className="m-0">
              <ActivityFeedList
                items={filterByCategory('needs_review')}
                onNavigate={handleNavigate}
              />
            </TabsContent>
            <TabsContent value="in_progress" className="m-0">
              <ActivityFeedList
                items={filterByCategory('in_progress')}
                onNavigate={handleNavigate}
              />
            </TabsContent>
            <TabsContent value="completed" className="m-0">
              <ActivityFeedList
                items={filterByCategory('completed')}
                onNavigate={handleNavigate}
              />
            </TabsContent>
          </Tabs>
        )}
      </PopoverContent>
    </Popover>
  );
}

export default ActivityFeed;
