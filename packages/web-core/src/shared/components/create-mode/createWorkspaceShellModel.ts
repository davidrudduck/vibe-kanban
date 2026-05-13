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

export function getSourceLinkedIssue({
  lockedLinkedIssue,
  draftLinkedIssue,
}: {
  lockedLinkedIssue: LinkedIssue | null;
  draftLinkedIssue: LinkedIssue | null;
}): LinkedIssue | null {
  return lockedLinkedIssue ?? draftLinkedIssue;
}

export function getEffectiveLinkedIssue({
  lockedToLinkedIssue,
  mode,
  sourceLinkedIssue,
  manualLinkedIssue,
}: {
  lockedToLinkedIssue: boolean;
  mode: CreateWorkspaceMode;
  sourceLinkedIssue: LinkedIssue | null;
  manualLinkedIssue: LinkedIssue | null;
}): LinkedIssue | null {
  if (lockedToLinkedIssue) {
    return sourceLinkedIssue;
  }

  if (mode !== 'link_task') {
    return null;
  }

  return manualLinkedIssue ?? sourceLinkedIssue;
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

export type WorkspaceCreateSubmissionState =
  | { status: 'pending' }
  | { status: 'created'; workspaceId: string };

export type WorkspaceCreateSubmissionReservation =
  | { status: 'reserved' }
  | { status: 'pending' }
  | { status: 'created'; workspaceId: string };

export function reserveWorkspaceCreateSubmission(
  registry: Map<string, WorkspaceCreateSubmissionState>,
  key: string | null | undefined
): WorkspaceCreateSubmissionReservation {
  if (!key) return { status: 'reserved' };

  const existing = registry.get(key);
  if (existing?.status === 'created') {
    return { status: 'created', workspaceId: existing.workspaceId };
  }
  if (existing?.status === 'pending') {
    return { status: 'pending' };
  }

  registry.set(key, { status: 'pending' });
  return { status: 'reserved' };
}

export function completeWorkspaceCreateSubmission(
  registry: Map<string, WorkspaceCreateSubmissionState>,
  key: string | null | undefined,
  workspaceId: string
) {
  if (!key) return;
  registry.set(key, { status: 'created', workspaceId });
}

export function releaseWorkspaceCreateSubmission(
  registry: Map<string, WorkspaceCreateSubmissionState>,
  key: string | null | undefined
) {
  if (!key) return;
  if (registry.get(key)?.status === 'pending') {
    registry.delete(key);
  }
}
