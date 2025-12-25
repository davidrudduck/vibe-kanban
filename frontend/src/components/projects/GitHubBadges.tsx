import { useTranslation } from 'react-i18next';
import { CircleDot, GitPullRequest } from 'lucide-react';

interface GitHubBadgesProject {
  github_enabled: boolean;
  github_open_issues: number;
  github_open_prs: number;
}

interface GitHubBadgesProps {
  project: GitHubBadgesProject;
  compact?: boolean;
  onClick?: () => void;
}

export function GitHubBadges({
  project,
  compact = false,
  onClick,
}: GitHubBadgesProps) {
  const { t } = useTranslation('projects');

  if (!project.github_enabled) {
    return null;
  }

  const handleClick = (e: React.MouseEvent) => {
    if (onClick) {
      e.stopPropagation();
      onClick();
    }
  };

  if (compact) {
    // Compact mode: just icons with counts
    return (
      <div
        className="flex items-center gap-2 text-xs text-muted-foreground cursor-pointer hover:text-foreground transition-colors"
        onClick={handleClick}
        title={t('github.settings')}
      >
        <span className="inline-flex items-center gap-0.5">
          <CircleDot className="h-3 w-3 text-blue-500" />
          {project.github_open_issues}
        </span>
        <span className="inline-flex items-center gap-0.5">
          <GitPullRequest className="h-3 w-3 text-green-500" />
          {project.github_open_prs}
        </span>
      </div>
    );
  }

  // Normal mode: badges with labels
  return (
    <div
      className="flex items-center gap-2 cursor-pointer"
      onClick={handleClick}
      title={t('github.settings')}
    >
      <span className="inline-flex items-center gap-1 px-2 py-0.5 text-xs font-medium rounded-full bg-blue-50 text-blue-700 border border-blue-200 dark:bg-blue-950 dark:text-blue-300 dark:border-blue-800">
        <CircleDot className="h-3 w-3" />
        {project.github_open_issues}
        <span className="hidden sm:inline">{t('github.openIssues')}</span>
      </span>
      <span className="inline-flex items-center gap-1 px-2 py-0.5 text-xs font-medium rounded-full bg-green-50 text-green-700 border border-green-200 dark:bg-green-950 dark:text-green-300 dark:border-green-800">
        <GitPullRequest className="h-3 w-3" />
        {project.github_open_prs}
        <span className="hidden sm:inline">{t('github.openPRs')}</span>
      </span>
    </div>
  );
}

export default GitHubBadges;
