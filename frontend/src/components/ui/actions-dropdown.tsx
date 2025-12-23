import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { MoreHorizontal } from 'lucide-react';
import type { TaskWithAttemptStatus, TaskAttempt } from 'shared/types';
import { useOpenInEditor } from '@/hooks/useOpenInEditor';
import { ArchiveTaskConfirmationDialog } from '@/components/dialogs/tasks/ArchiveTaskConfirmationDialog';
import { DeleteTaskConfirmationDialog } from '@/components/dialogs/tasks/DeleteTaskConfirmationDialog';
import { ViewProcessesDialog } from '@/components/dialogs/tasks/ViewProcessesDialog';
import { ViewRelatedTasksDialog } from '@/components/dialogs/tasks/ViewRelatedTasksDialog';
import { CreateAttemptDialog } from '@/components/dialogs/tasks/CreateAttemptDialog';
import { GitActionsDialog } from '@/components/dialogs/tasks/GitActionsDialog';
import { EditBranchNameDialog } from '@/components/dialogs/tasks/EditBranchNameDialog';
import { ShareDialog } from '@/components/dialogs/tasks/ShareDialog';
import { ReassignDialog } from '@/components/dialogs/tasks/ReassignDialog';
import { StopShareTaskDialog } from '@/components/dialogs/tasks/StopShareTaskDialog';
import { useProject } from '@/contexts/ProjectContext';
import { openTaskForm } from '@/lib/openTaskForm';

import { useNavigate } from 'react-router-dom';
import type { SharedTaskRecord } from '@/hooks/useProjectTasks';
import { useAuth, useTaskUsesSharedWorktree } from '@/hooks';
import { tasksApi } from '@/lib/api';
import { useState } from 'react';

interface ActionsDropdownProps {
  task?: TaskWithAttemptStatus | null;
  attempt?: TaskAttempt | null;
  sharedTask?: SharedTaskRecord;
  isOrgAdmin?: boolean;
}

export function ActionsDropdown({
  task,
  attempt,
  sharedTask,
  isOrgAdmin = false,
}: ActionsDropdownProps) {
  const { t } = useTranslation('tasks');
  const { projectId } = useProject();
  const openInEditor = useOpenInEditor(attempt?.id);
  const navigate = useNavigate();
  const { userId } = useAuth();

  const hasAttemptActions = Boolean(attempt);
  const hasTaskActions = Boolean(task);
  const isShared = Boolean(sharedTask);
  const isRemote = Boolean(task?.is_remote);

  // Check if this task uses a shared worktree (prevents subtask creation)
  const { usesSharedWorktree } = useTaskUsesSharedWorktree(task?.id);

  const handleEdit = (e: React.MouseEvent) => {
    e.stopPropagation();
    if (!projectId || !task) return;
    openTaskForm({ mode: 'edit', projectId, task });
  };

  const handleDuplicate = (e: React.MouseEvent) => {
    e.stopPropagation();
    if (!projectId || !task) return;
    openTaskForm({ mode: 'duplicate', projectId, initialTask: task });
  };

  const handleDelete = async (e: React.MouseEvent) => {
    e.stopPropagation();
    if (!projectId || !task) return;
    try {
      await DeleteTaskConfirmationDialog.show({
        task,
        projectId,
      });
    } catch {
      // User cancelled or error occurred
    }
  };

  const handleOpenInEditor = (e: React.MouseEvent) => {
    e.stopPropagation();
    if (!attempt?.id) return;
    openInEditor();
  };

  const handleViewProcesses = (e: React.MouseEvent) => {
    e.stopPropagation();
    if (!attempt?.id) return;
    ViewProcessesDialog.show({ attemptId: attempt.id });
  };

  const handleViewRelatedTasks = (e: React.MouseEvent) => {
    e.stopPropagation();
    if (!task?.id || !projectId) return;
    ViewRelatedTasksDialog.show({
      taskId: task.id,
      projectId,
      attemptId: attempt?.id,
      attempt: attempt ?? undefined,
      onNavigateToTask: (navTaskId: string) => {
        if (projectId) {
          navigate(`/projects/${projectId}/tasks/${navTaskId}/attempts/latest`);
        }
      },
    });
  };

  const handleCreateNewAttempt = (e: React.MouseEvent) => {
    e.stopPropagation();
    if (!task?.id) return;
    CreateAttemptDialog.show({
      taskId: task.id,
    });
  };

  const handleCreateSubtask = (e: React.MouseEvent) => {
    e.stopPropagation();
    if (!projectId || !task) return;
    openTaskForm({
      mode: 'subtask',
      projectId,
      parentTaskId: task.id,
      initialBaseBranch: attempt?.branch || attempt?.target_branch,
    });
  };

  const handleGitActions = (e: React.MouseEvent) => {
    e.stopPropagation();
    if (!attempt?.id || !task) return;
    GitActionsDialog.show({
      attemptId: attempt.id,
      task,
      projectId,
    });
  };

  const handleEditBranchName = (e: React.MouseEvent) => {
    e.stopPropagation();
    if (!attempt?.id) return;
    EditBranchNameDialog.show({
      attemptId: attempt.id,
      currentBranchName: attempt.branch,
    });
  };
  const handleShare = (e: React.MouseEvent) => {
    e.stopPropagation();
    if (!task || isShared) return;
    ShareDialog.show({ task });
  };

  const handleReassign = (e: React.MouseEvent) => {
    e.stopPropagation();
    if (!sharedTask) return;
    ReassignDialog.show({ sharedTask, isOrgAdmin });
  };

  const handleStopShare = (e: React.MouseEvent) => {
    e.stopPropagation();
    if (!sharedTask) return;
    StopShareTaskDialog.show({ sharedTask });
  };

  const [isUnarchiving, setIsUnarchiving] = useState(false);

  const handleArchive = async (e: React.MouseEvent) => {
    e.stopPropagation();
    if (!projectId || !task) return;
    try {
      await ArchiveTaskConfirmationDialog.show({
        task,
        projectId,
      });
    } catch {
      // User cancelled or error occurred
    }
  };

  const handleUnarchive = async (e: React.MouseEvent) => {
    e.stopPropagation();
    if (!task || isUnarchiving) return;
    setIsUnarchiving(true);
    try {
      await tasksApi.unarchive(task.id);
    } catch (err) {
      console.error('Failed to unarchive task:', err);
    } finally {
      setIsUnarchiving(false);
    }
  };

  const isAssignee = sharedTask?.assignee_user_id === userId;
  const isRemoteAssignee = task?.remote_assignee_user_id === userId;

  // Permission to modify (edit/delete) a task:
  // - Assignee of the shared task
  // - Assignee of the remote task
  // - Org admin
  // - For local tasks without shared/remote info, anyone can edit (preserve current behavior)
  const isLocalOnlyTask = !isShared && !isRemote;
  const canModifyTask =
    isLocalOnlyTask || isAssignee || isRemoteAssignee || isOrgAdmin;

  // For reassign: need both task and sharedTask, unless admin (admins can reassign shared-only tasks)
  const canReassign =
    Boolean(sharedTask) &&
    (Boolean(task) || isOrgAdmin) &&
    (isAssignee || isOrgAdmin);
  const canStopShare = Boolean(sharedTask) && (isAssignee || isOrgAdmin);
  // Show shared task actions section when we only have a sharedTask (no local task)
  const hasSharedOnlyActions =
    !hasTaskActions && Boolean(sharedTask) && isOrgAdmin;

  return (
    <>
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button
            variant="icon"
            aria-label="Actions"
            onPointerDown={(e) => e.stopPropagation()}
            onMouseDown={(e) => e.stopPropagation()}
            onClick={(e) => e.stopPropagation()}
          >
            <MoreHorizontal className="h-4 w-4" />
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end">
          {hasAttemptActions && (
            <>
              <DropdownMenuLabel>{t('actionsMenu.attempt')}</DropdownMenuLabel>
              <DropdownMenuItem
                disabled={!attempt?.id}
                onClick={handleOpenInEditor}
              >
                {t('actionsMenu.openInIde')}
              </DropdownMenuItem>
              <DropdownMenuItem
                disabled={!attempt?.id}
                onClick={handleViewProcesses}
              >
                {t('actionsMenu.viewProcesses')}
              </DropdownMenuItem>
              <DropdownMenuItem
                disabled={isRemote}
                onClick={handleCreateNewAttempt}
                title={
                  isRemote
                    ? t('actionsMenu.remoteTaskCannotExecute')
                    : undefined
                }
              >
                {t('actionsMenu.createNewAttempt')}
              </DropdownMenuItem>
              <DropdownMenuItem
                disabled={!attempt?.id || !task || isRemote}
                onClick={handleGitActions}
                title={
                  isRemote
                    ? t('actionsMenu.remoteTaskCannotExecute')
                    : undefined
                }
              >
                {t('actionsMenu.gitActions')}
              </DropdownMenuItem>
              <DropdownMenuItem
                disabled={!attempt?.id || isRemote}
                onClick={handleEditBranchName}
                title={
                  isRemote
                    ? t('actionsMenu.remoteTaskCannotExecute')
                    : undefined
                }
              >
                {t('actionsMenu.editBranchName')}
              </DropdownMenuItem>
              <DropdownMenuSeparator />
            </>
          )}

          {hasTaskActions && (
            <>
              <DropdownMenuLabel>{t('actionsMenu.task')}</DropdownMenuLabel>
              <DropdownMenuItem
                disabled={!task || isShared}
                onClick={handleShare}
              >
                {t('actionsMenu.share')}
              </DropdownMenuItem>
              <DropdownMenuItem
                disabled={!canReassign}
                onClick={handleReassign}
              >
                {t('actionsMenu.reassign')}
              </DropdownMenuItem>
              <DropdownMenuItem
                disabled={!canStopShare}
                onClick={handleStopShare}
                className="text-destructive"
              >
                {t('actionsMenu.stopShare')}
              </DropdownMenuItem>
              <DropdownMenuSeparator />
              <DropdownMenuItem
                disabled={!task?.id || !projectId}
                onClick={handleViewRelatedTasks}
              >
                {t('actionsMenu.viewRelatedTasks')}
              </DropdownMenuItem>
              <DropdownMenuItem
                disabled={!projectId || !task || isRemote || usesSharedWorktree}
                onClick={handleCreateSubtask}
                title={
                  isRemote
                    ? t('actionsMenu.remoteTaskCannotExecute')
                    : usesSharedWorktree
                      ? t(
                          'actionsMenu.sharedWorktreeNoSubtask',
                          'Cannot create subtasks for tasks using a shared worktree'
                        )
                      : undefined
                }
              >
                {t('actionsMenu.createSubtask')}
              </DropdownMenuItem>
              <DropdownMenuSeparator />
              {!task?.archived_at && (
                <DropdownMenuItem
                  disabled={!projectId || !canModifyTask || isRemote}
                  onClick={handleArchive}
                  title={
                    isRemote
                      ? t('actionsMenu.remoteTaskCannotExecute')
                      : undefined
                  }
                >
                  {t('actionsMenu.archive')}
                </DropdownMenuItem>
              )}
              {task?.archived_at && (
                <DropdownMenuItem
                  disabled={!canModifyTask || isRemote || isUnarchiving}
                  onClick={handleUnarchive}
                  title={
                    isRemote
                      ? t('actionsMenu.remoteTaskCannotExecute')
                      : undefined
                  }
                >
                  {t('actionsMenu.unarchive')}
                </DropdownMenuItem>
              )}
              <DropdownMenuItem
                disabled={!projectId || !canModifyTask}
                onClick={handleEdit}
              >
                {t('common:buttons.edit')}
              </DropdownMenuItem>
              <DropdownMenuItem disabled={!projectId} onClick={handleDuplicate}>
                {t('actionsMenu.duplicate')}
              </DropdownMenuItem>
              <DropdownMenuItem
                disabled={!projectId || !canModifyTask}
                onClick={handleDelete}
                className="text-destructive"
              >
                {t('common:buttons.delete')}
              </DropdownMenuItem>
            </>
          )}

          {hasSharedOnlyActions && (
            <>
              <DropdownMenuLabel>
                {t('actionsMenu.sharedTask')}
              </DropdownMenuLabel>
              <DropdownMenuItem
                disabled={!canReassign}
                onClick={handleReassign}
              >
                {t('actionsMenu.reassign')}
              </DropdownMenuItem>
              <DropdownMenuItem
                disabled={!canStopShare}
                onClick={handleStopShare}
                className="text-destructive"
              >
                {t('actionsMenu.stopShare')}
              </DropdownMenuItem>
            </>
          )}
        </DropdownMenuContent>
      </DropdownMenu>
    </>
  );
}
