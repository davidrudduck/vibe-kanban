import { Tabs, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { Monitor, Cloud, Layers } from 'lucide-react';
import { cn } from '@/lib/utils';

export type ProjectTypeFilter = 'all' | 'local' | 'swarm';

type Props = {
  value: ProjectTypeFilter;
  onChange: (value: ProjectTypeFilter) => void;
  /** Counts for each type */
  counts: {
    total: number;
    local: number;
    swarm: number;
  };
  className?: string;
};

/**
 * Filter tabs for project type selection.
 * Nordic Clean aesthetic - subtle pill tabs with icons.
 */
export function ProjectTypeFilterTabs({
  value,
  onChange,
  counts,
  className,
}: Props) {
  return (
    <Tabs
      value={value}
      onValueChange={(v) => onChange(v as ProjectTypeFilter)}
      className={cn('w-auto', className)}
    >
      <TabsList className="h-8 p-0.5">
        <TabsTrigger
          value="all"
          className="h-7 px-3 text-xs gap-1.5 data-[state=active]:bg-background"
        >
          <Layers className="h-3.5 w-3.5" />
          <span className="hidden sm:inline">All</span>
          <span className="text-muted-foreground font-normal">
            ({counts.total})
          </span>
        </TabsTrigger>
        <TabsTrigger
          value="local"
          className="h-7 px-3 text-xs gap-1.5 data-[state=active]:bg-background"
        >
          <Monitor className="h-3.5 w-3.5" />
          <span className="hidden sm:inline">Local</span>
          <span className="text-muted-foreground font-normal">
            ({counts.local})
          </span>
        </TabsTrigger>
        <TabsTrigger
          value="swarm"
          className="h-7 px-3 text-xs gap-1.5 data-[state=active]:bg-background"
        >
          <Cloud className="h-3.5 w-3.5" />
          <span className="hidden sm:inline">Swarm</span>
          <span className="text-muted-foreground font-normal">
            ({counts.swarm})
          </span>
        </TabsTrigger>
      </TabsList>
    </Tabs>
  );
}

export default ProjectTypeFilterTabs;
