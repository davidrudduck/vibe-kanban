import { useEffect } from 'react';
import { useUserSystem } from '@/shared/hooks/useUserSystem';
import { setFeedbackUrl } from '@/shared/actions';

export function FeedbackUrlSync() {
  const { config } = useUserSystem();

  useEffect(() => {
    setFeedbackUrl(config?.appearance?.links?.feedback_url ?? null);
  }, [config?.appearance?.links?.feedback_url]);

  return null;
}
