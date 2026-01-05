import { Badge } from '@/components/ui/badge';
import { Circle, Monitor } from 'lucide-react';
import type { MergedProject, CachedNodeStatus } from 'shared/types';
import { cn } from '@/lib/utils';

type Props = {
  project: MergedProject;
  /** Show only icons without text on very small screens */
  compact?: boolean;
};

/** Get status indicator color */
function getStatusColor(status: CachedNodeStatus): string {
  switch (status) {
    case 'online':
      return 'text-emerald-500 fill-emerald-500';
    case 'busy':
      return 'text-amber-500 fill-amber-500';
    case 'offline':
      return 'text-gray-400 fill-gray-400';
    case 'draining':
      return 'text-orange-500 fill-orange-500';
    case 'pending':
    default:
      return 'text-gray-300 fill-gray-300';
  }
}

/** Get OS icon component based on os string */
function OsIcon({ os, className }: { os: string | null | undefined; className?: string }) {
  // Apple logo for darwin/macOS
  if (os === 'darwin') {
    return (
      <svg
        viewBox="0 0 24 24"
        className={cn('h-3 w-3', className)}
        fill="currentColor"
      >
        <path d="M18.71 19.5c-.83 1.24-1.71 2.45-3.05 2.47-1.34.03-1.77-.79-3.29-.79-1.53 0-2 .77-3.27.82-1.31.05-2.3-1.32-3.14-2.53C4.25 17 2.94 12.45 4.7 9.39c.87-1.52 2.43-2.48 4.12-2.51 1.28-.02 2.5.87 3.29.87.78 0 2.26-1.07 3.81-.91.65.03 2.47.26 3.64 1.98-.09.06-2.17 1.28-2.15 3.81.03 3.02 2.65 4.03 2.68 4.04-.03.07-.42 1.44-1.38 2.83M13 3.5c.73-.83 1.94-1.46 2.94-1.5.13 1.17-.34 2.35-1.04 3.19-.69.85-1.83 1.51-2.95 1.42-.15-1.15.41-2.35 1.05-3.11z" />
      </svg>
    );
  }

  // Linux penguin for linux
  if (os === 'linux') {
    return (
      <svg
        viewBox="0 0 24 24"
        className={cn('h-3 w-3', className)}
        fill="currentColor"
      >
        <path d="M12.504 0c-.155 0-.311.003-.467.008-3.553.11-6.14 2.58-6.036 5.752.055 1.7.667 3.27 1.697 4.684l.008.01a.5.5 0 0 1-.006.684c-.125.139-.243.283-.356.432-.95 1.256-1.487 2.754-1.487 4.28 0 .31.026.621.078.93.287 1.712 1.29 3.148 2.72 3.936 1.09.603 2.355.906 3.686.906.406 0 .815-.029 1.22-.084.376-.05.753-.123 1.125-.22.793-.205 1.527-.518 2.17-.937 1.165-.761 2.004-1.825 2.38-3.06.136-.45.205-.912.205-1.37 0-.778-.214-1.55-.622-2.257-.18-.313-.399-.604-.65-.867l-.003-.003a.5.5 0 0 1 0-.708l.012-.012c1.122-1.154 1.807-2.71 1.888-4.314.075-1.516-.35-2.956-1.206-4.048C17.003 2.115 14.94.97 12.565.008A6.3 6.3 0 0 0 12.504 0Zm.182 2.017c.228.006.453.035.67.09 1.67.426 2.967 1.285 3.567 2.387.4.733.567 1.54.486 2.347-.083.834-.42 1.639-.97 2.337a.5.5 0 0 1-.75-.66c.431-.549.693-1.18.758-1.84.059-.596-.07-1.2-.376-1.76-.415-.758-1.46-1.456-2.83-1.806a3.287 3.287 0 0 0-.462-.063c-.16-.015-.319-.018-.475-.008-1.18.067-2.237.63-2.894 1.545-.54.753-.79 1.66-.704 2.558.09.938.488 1.832 1.12 2.524a.5.5 0 1 1-.724.69c-.786-.862-1.28-1.987-1.393-3.17-.1-1.042.185-2.095.808-2.965.794-1.11 2.077-1.796 3.511-1.874.06-.003.12-.005.178-.005Zm-.12 7.955c.17.003.34.02.509.052 1.052.204 1.961.77 2.553 1.589.432.598.67 1.295.67 1.977 0 .324-.048.656-.14.984-.28.99-.986 1.864-1.94 2.487-.544.355-1.168.618-1.84.774-.315.073-.636.122-.956.146-.339.025-.682.022-1.02-.006-.674-.057-1.318-.208-1.905-.445-.972-.393-1.773-1.01-2.298-1.756-.384-.546-.631-1.162-.722-1.803-.043-.307-.054-.619-.032-.93.038-.528.165-1.043.375-1.528.257-.591.634-1.133 1.108-1.591a.5.5 0 0 1 .696.72c-.396.384-.713.83-.934 1.314-.176.387-.288.8-.328 1.218-.033.341-.027.682.016 1.012.071.535.274 1.047.592 1.5.446.635 1.127 1.143 1.973 1.485.502.203 1.052.333 1.633.388.29.028.584.034.878.018.277-.015.556-.054.831-.115.567-.13 1.092-.353 1.54-.653.77-.517 1.335-1.19 1.56-1.987.067-.236.1-.476.1-.709 0-.517-.179-1.048-.505-1.5-.465-.645-1.23-1.1-2.141-1.277a2.7 2.7 0 0 0-.37-.042c-.186-.008-.37.001-.55.029-.426.066-.826.215-1.167.435a.5.5 0 0 1-.54-.842c.434-.28.925-.47 1.439-.555.223-.038.452-.054.68-.049Z" />
      </svg>
    );
  }

  // Windows logo for windows
  if (os === 'windows') {
    return (
      <svg
        viewBox="0 0 24 24"
        className={cn('h-3 w-3', className)}
        fill="currentColor"
      >
        <path d="M0 3.449L9.75 2.1v9.451H0m10.949-9.602L24 0v11.4H10.949M0 12.6h9.75v9.451L0 20.699M10.949 12.6H24V24l-12.9-1.801" />
      </svg>
    );
  }

  // Default: generic server/computer icon
  return <Monitor className={cn('h-3 w-3', className)} />;
}

/**
 * Location badges showing where a project exists (local and/or remote nodes).
 * Nordic Clean aesthetic - subtle badges with OS-specific icons.
 */
export function LocationBadges({ project, compact = false }: Props) {
  return (
    <div className="flex flex-wrap gap-1.5">
      {/* Local badge */}
      {project.has_local && (
        <Badge
          variant="outline"
          className={cn(
            'gap-1 font-normal',
            compact ? 'px-1.5 py-0 text-[10px]' : 'px-2 py-0.5 text-xs'
          )}
        >
          <Monitor className="h-3 w-3 text-muted-foreground" />
          {!compact && <span>local</span>}
        </Badge>
      )}

      {/* Remote node badges */}
      {project.nodes.map((node) => (
        <Badge
          key={node.node_id}
          variant="secondary"
          className={cn(
            'gap-1 font-normal',
            compact ? 'px-1.5 py-0 text-[10px]' : 'px-2 py-0.5 text-xs'
          )}
        >
          <OsIcon os={node.node_os} className="text-muted-foreground" />
          {!compact && <span>{node.node_short_name}</span>}
          <Circle className={cn('h-2 w-2', getStatusColor(node.node_status))} />
        </Badge>
      ))}
    </div>
  );
}

export default LocationBadges;
