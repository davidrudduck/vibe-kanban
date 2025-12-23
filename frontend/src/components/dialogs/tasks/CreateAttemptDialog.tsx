import { useState, useEffect, useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import { Label } from '@/components/ui/label';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import BranchSelector from '@/components/tasks/BranchSelector';
import { ExecutorProfileSelector } from '@/components/settings';
import { useAttemptCreation } from '@/hooks/useAttemptCreation';
import {
  useNavigateWithSearch,
  useTask,
  useBranches,
  useTaskAttempts,
  useAvailableNodes,
} from '@/hooks';
import { useProject } from '@/contexts/ProjectContext';
import { useUserSystem } from '@/components/ConfigProvider';
import { paths } from '@/lib/paths';
import NiceModal, { useModal } from '@ebay/nice-modal-react';
import { defineModal } from '@/lib/modals';
import type { ExecutorProfileId, BaseCodingAgent } from 'shared/types';
import { useKeySubmitTask, Scope } from '@/keyboard';
import { Server } from 'lucide-react';

export interface CreateAttemptDialogProps {
  taskId: string;
}

const CreateAttemptDialogImpl = NiceModal.create<CreateAttemptDialogProps>(
  ({ taskId }) => {
    const modal = useModal();
    const navigate = useNavigateWithSearch();
    const { projectId } = useProject();
    const { t } = useTranslation('tasks');
    const { profiles, config } = useUserSystem();
    const { createAttempt, isCreating, error } = useAttemptCreation({
      taskId,
      onSuccess: (attempt) => {
        if (projectId) {
          navigate(paths.attempt(projectId, taskId, attempt.id));
        }
      },
    });

    const [userSelectedProfile, setUserSelectedProfile] =
      useState<ExecutorProfileId | null>(null);
    const [userSelectedBranch, setUserSelectedBranch] = useState<string | null>(
      null
    );
    const [selectedNodeId, setSelectedNodeId] = useState<string | null>(null);
    const [useParentWorktree, setUseParentWorktree] = useState<boolean>(false);

    // Fetch available nodes for remote execution
    const { data: availableNodesData, isLoading: isLoadingNodes } =
      useAvailableNodes(taskId, { enabled: modal.visible });

    const availableNodes = useMemo(() => {
      return availableNodesData?.nodes ?? [];
    }, [availableNodesData]);

    // Show node selector only when there are remote nodes available
    const hasAvailableNodes = availableNodes.length > 0;

    const { data: branches = [], isLoading: isLoadingBranches } = useBranches(
      projectId,
      { enabled: modal.visible && !!projectId }
    );

    const { data: attempts = [], isLoading: isLoadingAttempts } =
      useTaskAttempts(taskId, {
        enabled: modal.visible,
        refetchInterval: 5000,
      });

    const { data: task, isLoading: isLoadingTask } = useTask(taskId, {
      enabled: modal.visible,
    });

    // Fetch parent task's attempts if this task has a parent
    const parentTaskId = task?.parent_task_id ?? undefined;

    // Get parent task's latest attempt to determine default branch
    const { data: parentAttempts = [], isLoading: isLoadingParentAttempts } =
      useTaskAttempts(parentTaskId, {
        enabled: modal.visible && !!parentTaskId,
      });

    const parentLatestAttempt = useMemo(() => {
      if (parentAttempts.length === 0) return null;
      return parentAttempts.reduce((latest, attempt) =>
        new Date(attempt.created_at) > new Date(latest.created_at)
          ? attempt
          : latest
      );
    }, [parentAttempts]);

    const latestAttempt = useMemo(() => {
      if (attempts.length === 0) return null;
      return attempts.reduce((latest, attempt) =>
        new Date(attempt.created_at) > new Date(latest.created_at)
          ? attempt
          : latest
      );
    }, [attempts]);

    useEffect(() => {
      if (!modal.visible) {
        setUserSelectedProfile(null);
        setUserSelectedBranch(null);
        setSelectedNodeId(null);
        setUseParentWorktree(false);
      }
    }, [modal.visible]);

    // Auto-enable "use parent worktree" for child tasks with valid parent attempts
    useEffect(() => {
      if (modal.visible && task?.parent_task_id && parentLatestAttempt?.container_ref) {
        setUseParentWorktree(true);
      }
    }, [modal.visible, task?.parent_task_id, parentLatestAttempt?.container_ref]);

    const defaultProfile: ExecutorProfileId | null = useMemo(() => {
      if (latestAttempt?.executor) {
        const lastExec = latestAttempt.executor as BaseCodingAgent;
        // If the last attempt used the same executor as the user's current preference,
        // we assume they want to use their preferred variant as well.
        // Otherwise, we default to the "default" variant (null) since we don't know
        // what variant they used last time (TaskAttempt doesn't store it).
        const variant =
          config?.executor_profile?.executor === lastExec
            ? config.executor_profile.variant
            : null;

        return {
          executor: lastExec,
          variant,
        };
      }
      return config?.executor_profile ?? null;
    }, [latestAttempt?.executor, config?.executor_profile]);

    const currentBranchName: string | null = useMemo(() => {
      return branches.find((b) => b.is_current)?.name ?? null;
    }, [branches]);

    const defaultBranch: string | null = useMemo(() => {
      return (
        parentLatestAttempt?.branch ??
        currentBranchName ??
        latestAttempt?.target_branch ??
        null
      );
    }, [
      parentLatestAttempt?.branch,
      currentBranchName,
      latestAttempt?.target_branch,
    ]);

    const effectiveProfile = userSelectedProfile ?? defaultProfile;
    const effectiveBranch = userSelectedBranch ?? defaultBranch;

    const isLoadingInitial =
      isLoadingBranches ||
      isLoadingAttempts ||
      isLoadingTask ||
      isLoadingParentAttempts ||
      isLoadingNodes;
    const canCreate = Boolean(
      effectiveProfile && effectiveBranch && !isCreating && !isLoadingInitial
    );

    const handleCreate = async () => {
      if (!effectiveProfile || !effectiveBranch) return;
      try {
        await createAttempt({
          profile: effectiveProfile,
          baseBranch: effectiveBranch,
          targetNodeId: selectedNodeId,
          useParentWorktree: useParentWorktree && !!task?.parent_task_id,
        });

        modal.hide();
      } catch (err) {
        console.error('Failed to create attempt:', err);
      }
    };

    const handleOpenChange = (open: boolean) => {
      if (!open) modal.hide();
    };

    useKeySubmitTask(handleCreate, {
      enabled: modal.visible && canCreate,
      scope: Scope.DIALOG,
      preventDefault: true,
    });

    return (
      <Dialog open={modal.visible} onOpenChange={handleOpenChange}>
        <DialogContent className="sm:max-w-[500px]">
          <DialogHeader>
            <DialogTitle>{t('createAttemptDialog.title')}</DialogTitle>
            <DialogDescription>
              {t('createAttemptDialog.description')}
            </DialogDescription>
          </DialogHeader>

          <div className="space-y-4 py-4">
            {profiles && (
              <div className="space-y-2">
                <ExecutorProfileSelector
                  profiles={profiles}
                  selectedProfile={effectiveProfile}
                  onProfileSelect={setUserSelectedProfile}
                  showLabel={true}
                />
              </div>
            )}

            <div className="space-y-2">
              <Label className="text-sm font-medium">
                {t('createAttemptDialog.baseBranch')}{' '}
                <span className="text-destructive">*</span>
              </Label>
              <BranchSelector
                branches={branches}
                selectedBranch={effectiveBranch}
                onBranchSelect={setUserSelectedBranch}
                placeholder={
                  isLoadingBranches
                    ? t('createAttemptDialog.loadingBranches')
                    : t('createAttemptDialog.selectBranch')
                }
              />
            </div>

            {/* Use parent worktree checkbox - shown only for child tasks with valid parent attempts */}
            {task?.parent_task_id && parentLatestAttempt?.container_ref && !parentLatestAttempt.worktree_deleted && (
              <div className="flex items-start space-x-3 pt-2">
                <Checkbox
                  id="use-parent-worktree"
                  checked={useParentWorktree}
                  onCheckedChange={setUseParentWorktree}
                />
                <div className="flex flex-col gap-1">
                  <Label htmlFor="use-parent-worktree" className="text-sm font-medium cursor-pointer">
                    {t('createAttemptDialog.useParentWorktree', 'Use parent worktree')}
                  </Label>
                  <span className="text-xs text-muted-foreground">
                    {t('createAttemptDialog.useParentWorktreeHelp',
                      'Continue work in same worktree and branch as parent task')}
                  </span>
                </div>
              </div>
            )}

            {/* Node selector - shown when there are available remote nodes */}
            {hasAvailableNodes && (
              <div className="space-y-2">
                <Label className="text-sm font-medium flex items-center gap-2">
                  <Server className="h-4 w-4" />
                  {t('createAttemptDialog.targetNode', 'Target Node')}
                </Label>
                <Select
                  value={selectedNodeId ?? 'local'}
                  onValueChange={(value) =>
                    setSelectedNodeId(value === 'local' ? null : value)
                  }
                >
                  <SelectTrigger>
                    <SelectValue
                      placeholder={t(
                        'createAttemptDialog.selectNode',
                        'Select node'
                      )}
                    />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="local">
                      {t('createAttemptDialog.localNode', 'Local (this node)')}
                    </SelectItem>
                    {availableNodes.map((node) => (
                      <SelectItem key={node.node_id} value={node.node_id}>
                        <span className="flex items-center gap-2">
                          <span
                            className={`h-2 w-2 rounded-full ${
                              node.node_status === 'online'
                                ? 'bg-green-500'
                                : 'bg-gray-400'
                            }`}
                          />
                          {node.node_name}
                        </span>
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
                <p className="text-xs text-muted-foreground">
                  {t(
                    'createAttemptDialog.nodeHelp',
                    'Select where to run this task attempt'
                  )}
                </p>
              </div>
            )}

            {error && (
              <div className="text-sm text-destructive">
                {t('createAttemptDialog.error')}
              </div>
            )}
          </div>

          <DialogFooter>
            <Button
              variant="outline"
              onClick={() => modal.hide()}
              disabled={isCreating}
            >
              {t('common:buttons.cancel')}
            </Button>
            <Button onClick={handleCreate} disabled={!canCreate}>
              {isCreating
                ? t('createAttemptDialog.creating')
                : t('createAttemptDialog.start')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    );
  }
);

export const CreateAttemptDialog = defineModal<CreateAttemptDialogProps, void>(
  CreateAttemptDialogImpl
);
