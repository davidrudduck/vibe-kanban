import { useMutation, useQueryClient } from '@tanstack/react-query';
import { workspacesApi } from '@/shared/lib/api';
import type {
  CreateAndStartWorkspaceRequest,
  CreateAndStartWorkspaceResponse,
  CreateWorkspaceApiRequest,
  Workspace,
} from 'shared/types';
import { workspaceSummaryKeys } from '@/shared/hooks/workspaceSummaryKeys';

interface CreateWorkspaceParams {
  data: CreateAndStartWorkspaceRequest;
}

interface CreateWorkspaceResult {
  workspace: CreateAndStartWorkspaceResponse['workspace'];
  linkErrorMessage?: string;
}

export function useCreateWorkspace() {
  const queryClient = useQueryClient();

  const createWorkspace = useMutation({
    mutationFn: async ({
      data,
    }: CreateWorkspaceParams): Promise<CreateWorkspaceResult> => {
      const { workspace, link_warning } =
        await workspacesApi.createAndStart(data);

      return { workspace, linkErrorMessage: link_warning ?? undefined };
    },
    onSuccess: () => {
      // Invalidate workspace summaries so they refresh with the new workspace included
      queryClient.invalidateQueries({ queryKey: workspaceSummaryKeys.all });
      // Ensure create-mode defaults refetch the latest session/model selection.
      queryClient.invalidateQueries({ queryKey: ['workspaceCreateDefaults'] });
    },
    onError: (err) => {
      console.error('Failed to create workspace:', err);
    },
  });

  const createDraftWorkspace = useMutation({
    mutationFn: async (
      data: Omit<CreateWorkspaceApiRequest, 'is_draft'>
    ): Promise<Workspace> => {
      return workspacesApi.createDraft(data);
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: workspaceSummaryKeys.all });
    },
    onError: (err) => {
      console.error('Failed to save draft workspace:', err);
    },
  });

  return { createWorkspace, createDraftWorkspace };
}
