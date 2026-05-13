import { splitMessageToTitleDescription } from '@/shared/lib/string';
import type {
  CreateWorkspaceMode,
  LinkedIssue,
} from '@/shared/types/createMode';

export function getCreateWorkspaceModeState({
  linkedIssue,
}: {
  linkedIssue: LinkedIssue | null;
}): {
  initialMode: CreateWorkspaceMode;
  lockedToLinkedIssue: boolean;
} {
  return {
    initialMode: linkedIssue ? 'link_task' : 'new_task',
    lockedToLinkedIssue: !!linkedIssue,
  };
}

export function getWorkspaceNameForSubmit({
  workspaceName,
  linkedIssueTitle,
  message,
  fallback = null,
}: {
  workspaceName: string;
  linkedIssueTitle?: string | null;
  message: string;
  fallback?: string | null;
}): string | null {
  const explicitName = workspaceName.trim();
  if (explicitName) return explicitName;

  const issueTitle = linkedIssueTitle?.trim();
  if (issueTitle) return issueTitle;

  const { title } = splitMessageToTitleDescription(message);
  return title || fallback;
}

export function canSaveWorkspaceDraft({
  workspaceName,
  linkedIssueTitle,
  message,
}: {
  workspaceName: string;
  linkedIssueTitle?: string | null;
  message: string;
}): boolean {
  return (
    workspaceName.trim().length > 0 ||
    !!linkedIssueTitle?.trim() ||
    message.trim().length > 0
  );
}
