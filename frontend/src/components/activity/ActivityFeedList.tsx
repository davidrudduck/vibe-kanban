import ActivityFeedItem from './ActivityFeedItem';
import type { ActivityFeedItem as ActivityFeedItemType } from 'shared/types';

interface Props {
  items: ActivityFeedItemType[];
  onNavigate?: () => void;
  onDismiss?: (taskId: string) => void;
  onUndismiss?: (taskId: string) => void;
}

function ActivityFeedList({ items, onNavigate, onDismiss, onUndismiss }: Props) {
  if (items.length === 0) {
    return (
      <div className="px-3 py-6 text-center text-muted-foreground text-sm">
        No activity
      </div>
    );
  }

  return (
    <div className="max-h-80 overflow-y-auto">
      {items.map((item) => (
        <ActivityFeedItem
          key={`${item.task_id}-${item.category}`}
          item={item}
          onNavigate={onNavigate}
          onDismiss={onDismiss}
          onUndismiss={onUndismiss}
        />
      ))}
    </div>
  );
}

export default ActivityFeedList;
