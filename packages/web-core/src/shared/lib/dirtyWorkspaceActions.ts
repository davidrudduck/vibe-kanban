import type { QueryClient } from '@tanstack/react-query';
import type { Workspace } from 'shared/types';
import { DirtyWorkspaceDialog } from '@/shared/dialogs/workspaces/DirtyWorkspaceDialog';
import type { DirtyWorkspaceOperation } from '@/shared/dialogs/workspaces/DirtyWorkspaceDialog';
import { CreatePRDialog } from '@/shared/dialogs/command-bar/CreatePRDialog';
import { workspacesApi } from '@/shared/lib/api';
import { workspaceRecordKeys } from '@/shared/hooks/useWorkspaceRecord';
import { workspaceRepoKeys } from '@/shared/hooks/useWorkspaceRepo';
import { repoBranchKeys } from '@/shared/hooks/useRepoBranches';

type RetryCleanup = (force: boolean) => Promise<void>;

interface HandleDirtyWorkspaceOptions {
  workspaceId: string;
  operation: DirtyWorkspaceOperation;
  branchName?: string | null;
  issueIdentifier?: string;
  queryClient: QueryClient;
  retry: RetryCleanup;
}

const defaultCommitMessage = (
  operation: DirtyWorkspaceOperation,
  branchName?: string | null
) => {
  const branch = branchName?.trim();
  const suffix = branch ? ` for ${branch}` : '';
  return operation === 'archive'
    ? `Save workspace changes before archiving${suffix}`
    : `Save workspace changes before deleting${suffix}`;
};

const invalidateGitWorkspaceState = (
  queryClient: QueryClient,
  workspaceId: string,
  repoIds: string[]
) => {
  queryClient.invalidateQueries({ queryKey: ['branch-status'] });
  queryClient.invalidateQueries({ queryKey: ['branchStatus', workspaceId] });
  queryClient.invalidateQueries({
    queryKey: workspaceRecordKeys.byId(workspaceId),
  });
  queryClient.invalidateQueries({
    queryKey: workspaceRepoKeys.byWorkspace(workspaceId),
  });
  for (const repoId of repoIds) {
    queryClient.invalidateQueries({
      queryKey: repoBranchKeys.byRepo(repoId),
    });
  }
};

const openPullRequestDialogs = async (
  workspace: Workspace,
  repoIds: string[],
  issueIdentifier?: string
): Promise<boolean> => {
  const repos = await workspacesApi.getRepos(workspace.id);
  for (const repoId of repoIds) {
    const repo = repos.find((item) => item.id === repoId);
    const result = await CreatePRDialog.show({
      attempt: workspace,
      repoId,
      targetBranch: repo?.target_branch,
      issueIdentifier,
    });

    if (!result.success && result.error) {
      throw new Error(result.error);
    }
    if (!result.success) {
      return false;
    }
  }
  return true;
};

export async function handleDirtyWorkspaceAction({
  workspaceId,
  operation,
  branchName,
  issueIdentifier,
  queryClient,
  retry,
}: HandleDirtyWorkspaceOptions): Promise<boolean> {
  const result = await DirtyWorkspaceDialog.show({
    operation,
    defaultCommitMessage: defaultCommitMessage(operation, branchName),
  });

  if (!result || result.action === 'cancel') {
    return false;
  }

  if (result.action === 'continue') {
    await retry(true);
    return true;
  }

  const commitResult = await workspacesApi.commit(workspaceId, {
    message: result.message,
  });
  const committedRepoIds = commitResult.committed_repo_ids;

  if (result.push) {
    for (const repoId of committedRepoIds) {
      const pushResult = await workspacesApi.push(workspaceId, {
        repo_id: repoId,
      });
      if (!pushResult.success) {
        throw new Error(pushResult.message || 'Failed to push changes');
      }
    }
  }

  if (result.createPr && committedRepoIds.length > 0) {
    const workspace = await workspacesApi.get(workspaceId);
    const createdPullRequests = await openPullRequestDialogs(
      workspace,
      committedRepoIds,
      issueIdentifier
    );
    if (!createdPullRequests) {
      invalidateGitWorkspaceState(queryClient, workspaceId, committedRepoIds);
      return false;
    }
  }

  invalidateGitWorkspaceState(queryClient, workspaceId, committedRepoIds);
  await retry(false);
  return true;
}
