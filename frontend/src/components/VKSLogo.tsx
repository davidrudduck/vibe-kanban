import { cn } from '@/lib/utils';

interface VKSLogoProps {
  /** Additional CSS classes */
  className?: string;
  /** Always show full "VK-SWARM" text regardless of viewport (default: false) */
  alwaysFull?: boolean;
}

/**
 * VK-Swarm responsive logo component.
 *
 * - Mobile (< 640px): Shows "VKS"
 * - Tablet/Desktop (>= 640px): Shows "VK-SWARM"
 *
 * Uses code font for terminal aesthetic and primary color for "VK" prefix.
 */
export function VKSLogo({ className, alwaysFull = false }: VKSLogoProps) {
  return (
    <div
      className={cn(
        'font-code font-bold tracking-tight select-none',
        className
      )}
      aria-label="VK-Swarm"
    >
      {/* Mobile version: VKS */}
      {!alwaysFull && (
        <span className="sm:hidden">
          <span className="text-primary">VK</span>
          <span className="text-foreground">S</span>
        </span>
      )}
      {/* Tablet/Desktop version: VK-SWARM */}
      <span className={alwaysFull ? '' : 'hidden sm:inline'}>
        <span className="text-primary">VK</span>
        <span className="text-foreground">-SWARM</span>
      </span>
    </div>
  );
}

/**
 * VKS Icon - simple "VK" badge for compact spaces
 */
export function VKSIcon({ className }: { className?: string }) {
  return (
    <div
      className={cn(
        'font-code font-bold tracking-tight select-none',
        className
      )}
      aria-label="VK-Swarm"
    >
      <span className="text-primary">VK</span>
    </div>
  );
}

export default VKSLogo;
