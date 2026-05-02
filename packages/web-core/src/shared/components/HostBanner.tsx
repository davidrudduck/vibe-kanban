import { useUserSystem } from '@/shared/hooks/useUserSystem';
import { cn } from '@/shared/lib/utils';

function getOsInfo(): string {
  // Prefer modern userAgentData API (Chrome/Edge)
  if (typeof navigator !== 'undefined') {
    const uad = (
      navigator as Navigator & { userAgentData?: { platform?: string } }
    ).userAgentData;
    if (uad?.platform) return uad.platform;
    // Fallback: extract OS from userAgent string
    const ua = navigator.userAgent;
    if (ua.includes('Mac OS X')) return 'macOS';
    if (ua.includes('Windows')) return 'Windows';
    if (ua.includes('Linux')) return 'Linux';
    if (ua.includes('Android')) return 'Android';
    if (ua.includes('iPhone') || ua.includes('iPad')) return 'iOS';
  }
  return 'Unknown OS';
}

export function HostBanner() {
  const { config } = useUserSystem();
  const bannerConfig = config?.appearance?.host_banner;

  if (!bannerConfig) return null;

  const { show_hostname, show_os_info, env_label } = bannerConfig;

  // Don't render if nothing is configured to show
  if (!show_hostname && !show_os_info && !env_label) return null;

  const hostname = show_hostname
    ? typeof window !== 'undefined'
      ? window.location.hostname
      : 'localhost'
    : null;
  const osInfo = show_os_info ? getOsInfo() : null;

  return (
    <div
      className={cn(
        'flex items-center gap-2 px-3 py-0.5 text-xs text-normal border-b border-border bg-secondary/50',
        'flex-wrap min-h-0'
      )}
    >
      <span className="text-brand">●</span>
      {hostname && <span className="text-high">{hostname}</span>}
      {osInfo && <span className="text-low">{osInfo}</span>}
      {env_label && (
        <span className="px-1.5 py-0.5 rounded bg-brand/15 text-brand font-medium text-[10px]">
          {env_label}
        </span>
      )}
    </div>
  );
}
