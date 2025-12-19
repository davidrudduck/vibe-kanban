import { useNavigate } from 'react-router-dom';
import { formatDistanceToNow } from 'date-fns';
import { cn } from '@/lib/utils';
import { Badge } from '@/components/ui/badge';
import type { ActivityFeedItem as ActivityFeedItemType } from 'shared/types';

interface Props {
  item: ActivityFeedItemType;
  onNavigate?: () => void;
}

function ActivityFeedItem({ item, onNavigate }: Props) {
  const navigate = useNavigate();

  const handleClick = () => {
    navigate(`/projects/${item.project_id}/tasks/${item.task_id}/attempts/latest`);
    onNavigate?.();
  };

  const categoryColors = {
    needs_review: 'bg-amber-100 text-amber-800 dark:bg-amber-900 dark:text-amber-200',
    in_progress: 'bg-blue-100 text-blue-800 dark:bg-blue-900 dark:text-blue-200',
    completed: 'bg-green-100 text-green-800 dark:bg-green-900 dark:text-green-200',
  };

  const categoryLabels = {
    needs_review: 'Review',
    in_progress: 'Running',
    completed: 'Done',
  };

  return (
    <div
      onClick={handleClick}
      className={cn(
        'flex flex-col gap-1 px-3 py-2 cursor-pointer hover:bg-muted/50 border-b last:border-b-0',
      )}
    >
      <div className="flex items-center justify-between gap-2">
        <span className="font-medium text-sm truncate flex-1">
          {item.task_title}
        </span>
        <Badge
          variant="secondary"
          className={cn('text-xs shrink-0', categoryColors[item.category])}
        >
          {categoryLabels[item.category]}
        </Badge>
      </div>
      <div className="flex items-center justify-between gap-2 text-xs text-muted-foreground">
        <span className="truncate">{item.project_name}</span>
        <span className="shrink-0">
          {formatDistanceToNow(new Date(item.activity_at), { addSuffix: true })}
        </span>
      </div>
    </div>
  );
}

export default ActivityFeedItem;
