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
import {
  MoreHorizontal,
  ExternalLink,
  Activity,
  Link2,
  Plus,
  GitBranch,
  GitMerge,
  Tag,
  UserPlus,
  Archive,
  ArchiveRestore,
  Pencil,
  Copy,
  Trash2,
  X,
  Play,
  Eye,
  CheckCircle,
  XCircle,
  Circle,
  HardDrive,
  FolderX,
} from 'lucide-react';
import type { TaskWithAttemptStatus, TaskAttempt, TaskStatus } from 'shared/types';
import { useOpenInEditor } from '@/hooks/useOpenInEditor';
import { ArchiveTaskConfirmationDialog } from '@/components/dialogs/tasks/ArchiveTaskConfirmationDialog';
import { CleanupWorktreeConfirmationDialog } from '@/components/dialogs/tasks/CleanupWorktreeConfirmationDialog';
import { DeleteTaskConfirmationDialog } from '@/components/dialogs/tasks/DeleteTaskConfirmationDialog';
import { ViewProcessesDialog } from '@/components/dialogs/tasks/ViewProcessesDialog';
import { ViewRelatedTasksDialog } from '@/components/dialogs/tasks/ViewRelatedTasksDialog';
import { CreateAttemptDialog } from '@/components/dialogs/tasks/CreateAttemptDialog';
import { GitActionsDialog } from '@/components/dialogs/tasks/GitActionsDialog';
import { EditBranchNameDialog } from '@/components/dialogs/tasks/EditBranchNameDialog';
import { ReassignDialog } from '@/components/dialogs/tasks/ReassignDialog';
import { useProject } from '@/contexts/ProjectContext';
import { getStatusCallback } from '@/contexts/TaskOptimisticContext';
import { openTaskForm } from '@/lib/openTaskForm';

import { useNavigate } from 'react-router-dom';
import type { SharedTaskRecord } from '@/hooks/useProjectTasks';
import { useAuth, useTaskUsesSharedWorktree, useIsMobile } from '@/hooks';
import { useAttemptCleanupMutations } from '@/hooks/useAttemptCleanupMutations';
import { tasksApi } from '@/lib/api';
import { useState, useEffect, useCallback } from 'react';
import { createPortal } from 'react-dom';
import { motion, AnimatePresence, useDragControls } from 'framer-motion';
import type { PanInfo } from 'framer-motion';
import { cn } from '@/lib/utils';

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
  const isMobile = useIsMobile();

  // Worktree cleanup mutations
  const { cleanupWorktree, purgeArtifacts } = useAttemptCleanupMutations({
    onCleanupSuccess: () => {
      // Could show a toast here
    },
    onPurgeSuccess: (result) => {
      // Could show a toast with freed bytes
      console.info(
        `Purged ${result.purged_dirs.join(', ')} - freed ${result.freed_bytes} bytes`
      );
    },
  });

  // Mobile sheet state
  const [mobileSheetOpen, setMobileSheetOpen] = useState(false);
  const dragControls = useDragControls();

  const closeMobileSheet = useCallback(() => {
    setMobileSheetOpen(false);
  }, []);

  // Handle drag end to determine if should close
  const handleDragEnd = useCallback(
    (_event: MouseEvent | TouchEvent | PointerEvent, info: PanInfo) => {
      if (info.offset.y > 100 || info.velocity.y > 500) {
        closeMobileSheet();
      }
    },
    [closeMobileSheet]
  );

  // Handle escape key for mobile sheet
  useEffect(() => {
    if (!mobileSheetOpen) return;

    const handleEscape = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        closeMobileSheet();
      }
    };

    document.addEventListener('keydown', handleEscape);
    return () => document.removeEventListener('keydown', handleEscape);
  }, [mobileSheetOpen, closeMobileSheet]);

  // Prevent body scroll when mobile sheet is open
  useEffect(() => {
    if (!mobileSheetOpen) return;

    const originalOverflow = document.body.style.overflow;
    document.body.style.overflow = 'hidden';
    return () => {
      document.body.style.overflow = originalOverflow;
    };
  }, [mobileSheetOpen]);

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

  const handleReassign = (e: React.MouseEvent) => {
    e.stopPropagation();
    if (!sharedTask) return;
    ReassignDialog.show({ sharedTask, isOrgAdmin });
  };

  const handleCleanupWorktree = async (e: React.MouseEvent) => {
    e.stopPropagation();
    if (!attempt?.id) return;
    try {
      await CleanupWorktreeConfirmationDialog.show({
        attemptId: attempt.id,
        onConfirm: async () => {
          await cleanupWorktree.mutateAsync(attempt.id);
        },
      });
    } catch {
      // User cancelled or error occurred
    }
  };

  const handlePurgeArtifacts = (e: React.MouseEvent) => {
    e.stopPropagation();
    if (!attempt?.id) return;
    purgeArtifacts.mutate(attempt.id);
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

  const [isUpdatingStatus, setIsUpdatingStatus] = useState(false);

  const handleStatusChange = async (
    e: React.MouseEvent,
    newStatus: TaskStatus
  ) => {
    e.stopPropagation();
    if (!task || !projectId || isUpdatingStatus) return;

    setIsUpdatingStatus(true);
    try {
      // Call the API to update status - pass null for all other fields
      await tasksApi.update(task.id, {
        title: null,
        description: null,
        status: newStatus,
        parent_task_id: null,
        image_ids: null,
      });
      // Optimistically update the local state
      const statusCallback = getStatusCallback(projectId);
      if (statusCallback) {
        statusCallback(task.id, newStatus);
      }
    } catch (err) {
      console.error('Failed to update task status:', err);
    } finally {
      setIsUpdatingStatus(false);
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
  // Show shared task actions section when we only have a sharedTask (no local task)
  const hasSharedOnlyActions =
    !hasTaskActions && Boolean(sharedTask) && isOrgAdmin;

  // Mobile menu item component with 48px touch target
  const MobileMenuItem = ({
    icon: Icon,
    label,
    onClick,
    disabled,
    destructive,
  }: {
    icon: React.ComponentType<{ className?: string }>;
    label: string;
    onClick: (e: React.MouseEvent) => void;
    disabled?: boolean;
    destructive?: boolean;
  }) => (
    <button
      className={cn(
        'flex items-center w-full min-h-[48px] px-4 py-3 text-left transition-colors',
        'hover:bg-accent active:bg-accent/80',
        disabled && 'opacity-50 pointer-events-none',
        destructive && 'text-destructive'
      )}
      onClick={(e) => {
        onClick(e);
        closeMobileSheet();
      }}
      disabled={disabled}
    >
      <Icon className="mr-3 h-5 w-5 flex-shrink-0" />
      <span className="text-sm font-medium">{label}</span>
    </button>
  );

  // Mobile section label component
  const MobileSectionLabel = ({ children }: { children: React.ReactNode }) => (
    <div className="px-4 py-2 text-xs font-semibold text-muted-foreground uppercase tracking-wide">
      {children}
    </div>
  );

  // Mobile separator component
  const MobileSeparator = () => (
    <div className="h-px bg-border my-1" />
  );

  // Trigger button (shared between mobile and desktop)
  const triggerButton = (
    <Button
      variant="icon"
      aria-label="Actions"
      onPointerDown={(e) => e.stopPropagation()}
      onMouseDown={(e) => e.stopPropagation()}
      onClick={(e) => {
        e.stopPropagation();
        if (isMobile) {
          setMobileSheetOpen(true);
        }
      }}
    >
      <MoreHorizontal className="h-4 w-4" />
    </Button>
  );

  // Mobile: Bottom sheet
  if (isMobile) {
    return (
      <>
        {triggerButton}

        {createPortal(
          <AnimatePresence>
            {mobileSheetOpen && (
              <>
                {/* Backdrop */}
                <motion.div
                  className="fixed inset-0 z-50 bg-black/50"
                  initial={{ opacity: 0 }}
                  animate={{ opacity: 1 }}
                  exit={{ opacity: 0 }}
                  onClick={closeMobileSheet}
                />

                {/* Sheet */}
                <motion.div
                  className="fixed bottom-0 left-0 right-0 z-50 bg-popover text-popover-foreground rounded-t-xl shadow-lg max-h-[85vh] overflow-hidden"
                  initial={{ y: '100%' }}
                  animate={{ y: 0 }}
                  exit={{ y: '100%' }}
                  transition={{ type: 'spring', damping: 30, stiffness: 300 }}
                  drag="y"
                  dragControls={dragControls}
                  dragConstraints={{ top: 0, bottom: 0 }}
                  dragElastic={{ top: 0, bottom: 0.5 }}
                  onDragEnd={handleDragEnd}
                >
                {/* Drag handle */}
                <div
                  className="flex justify-center py-3 cursor-grab active:cursor-grabbing touch-none"
                  onPointerDown={(e) => dragControls.start(e)}
                >
                  <div className="w-10 h-1 bg-muted-foreground/30 rounded-full" />
                </div>

                {/* Content */}
                <div className="overflow-y-auto max-h-[calc(85vh-44px)] pb-safe">
                  {hasAttemptActions && (
                    <>
                      <MobileSectionLabel>
                        {t('actionsMenu.attempt')}
                      </MobileSectionLabel>
                      <MobileMenuItem
                        icon={ExternalLink}
                        label={t('actionsMenu.openInIde')}
                        onClick={handleOpenInEditor}
                        disabled={!attempt?.id}
                      />
                      <MobileMenuItem
                        icon={Activity}
                        label={t('actionsMenu.viewProcesses')}
                        onClick={handleViewProcesses}
                        disabled={!attempt?.id}
                      />
                      <MobileMenuItem
                        icon={Plus}
                        label={t('actionsMenu.createNewAttempt')}
                        onClick={handleCreateNewAttempt}
                        disabled={isRemote}
                      />
                      <MobileMenuItem
                        icon={GitMerge}
                        label={t('actionsMenu.gitActions')}
                        onClick={handleGitActions}
                        disabled={!attempt?.id || !task || isRemote}
                      />
                      <MobileMenuItem
                        icon={Tag}
                        label={t('actionsMenu.editBranchName')}
                        onClick={handleEditBranchName}
                        disabled={!attempt?.id || isRemote}
                      />
                      <MobileSeparator />
                      <MobileMenuItem
                        icon={HardDrive}
                        label={t('actionsMenu.purgeArtifacts', 'Purge Build Artifacts')}
                        onClick={handlePurgeArtifacts}
                        disabled={
                          !attempt?.id ||
                          isRemote ||
                          attempt.worktree_deleted ||
                          purgeArtifacts.isPending
                        }
                      />
                      <MobileMenuItem
                        icon={FolderX}
                        label={t('actionsMenu.cleanupWorktree', 'Delete Worktree')}
                        onClick={handleCleanupWorktree}
                        disabled={
                          !attempt?.id ||
                          isRemote ||
                          attempt.worktree_deleted ||
                          cleanupWorktree.isPending
                        }
                        destructive
                      />
                      <MobileSeparator />
                    </>
                  )}

                  {hasTaskActions && (
                    <>
                      <MobileSectionLabel>
                        {t('actionsMenu.task')}
                      </MobileSectionLabel>
                      {/* Quick status change actions */}
                      {task?.status !== 'inprogress' && (
                        <MobileMenuItem
                          icon={Play}
                          label={t('actionsMenu.moveToInProgress')}
                          onClick={(e) => handleStatusChange(e, 'inprogress')}
                          disabled={!canModifyTask || isUpdatingStatus}
                        />
                      )}
                      {task?.status !== 'inreview' && (
                        <MobileMenuItem
                          icon={Eye}
                          label={t('actionsMenu.moveToInReview')}
                          onClick={(e) => handleStatusChange(e, 'inreview')}
                          disabled={!canModifyTask || isUpdatingStatus}
                        />
                      )}
                      {task?.status !== 'done' && (
                        <MobileMenuItem
                          icon={CheckCircle}
                          label={t('actionsMenu.moveToDone')}
                          onClick={(e) => handleStatusChange(e, 'done')}
                          disabled={!canModifyTask || isUpdatingStatus}
                        />
                      )}
                      {task?.status !== 'cancelled' && (
                        <MobileMenuItem
                          icon={XCircle}
                          label={t('actionsMenu.cancelTask')}
                          onClick={(e) => handleStatusChange(e, 'cancelled')}
                          disabled={!canModifyTask || isUpdatingStatus}
                        />
                      )}
                      {task?.status !== 'todo' && (
                        <MobileMenuItem
                          icon={Circle}
                          label={t('actionsMenu.moveToTodo')}
                          onClick={(e) => handleStatusChange(e, 'todo')}
                          disabled={!canModifyTask || isUpdatingStatus}
                        />
                      )}
                      <MobileSeparator />
                      <MobileMenuItem
                        icon={UserPlus}
                        label={t('actionsMenu.reassign')}
                        onClick={handleReassign}
                        disabled={!canReassign}
                      />
                      <MobileSeparator />
                      <MobileMenuItem
                        icon={Link2}
                        label={t('actionsMenu.viewRelatedTasks')}
                        onClick={handleViewRelatedTasks}
                        disabled={!task?.id || !projectId}
                      />
                      <MobileMenuItem
                        icon={GitBranch}
                        label={t('actionsMenu.createSubtask')}
                        onClick={handleCreateSubtask}
                        disabled={!projectId || !task || isRemote || usesSharedWorktree}
                      />
                      <MobileSeparator />
                      {!task?.archived_at && (
                        <MobileMenuItem
                          icon={Archive}
                          label={t('actionsMenu.archive')}
                          onClick={handleArchive}
                          disabled={!projectId || !canModifyTask || isRemote}
                        />
                      )}
                      {task?.archived_at && (
                        <MobileMenuItem
                          icon={ArchiveRestore}
                          label={t('actionsMenu.unarchive')}
                          onClick={handleUnarchive}
                          disabled={!canModifyTask || isRemote || isUnarchiving}
                        />
                      )}
                      <MobileMenuItem
                        icon={Pencil}
                        label={t('common:buttons.edit')}
                        onClick={handleEdit}
                        disabled={!projectId || !canModifyTask}
                      />
                      <MobileMenuItem
                        icon={Copy}
                        label={t('actionsMenu.duplicate')}
                        onClick={handleDuplicate}
                        disabled={!projectId}
                      />
                      <MobileMenuItem
                        icon={Trash2}
                        label={t('common:buttons.delete')}
                        onClick={handleDelete}
                        disabled={!projectId || !canModifyTask}
                        destructive
                      />
                    </>
                  )}

                  {hasSharedOnlyActions && (
                    <>
                      <MobileSectionLabel>
                        {t('actionsMenu.sharedTask')}
                      </MobileSectionLabel>
                      <MobileMenuItem
                        icon={UserPlus}
                        label={t('actionsMenu.reassign')}
                        onClick={handleReassign}
                        disabled={!canReassign}
                      />
                    </>
                  )}

                  {/* Cancel button */}
                  <div className="p-4 border-t border-border mt-2">
                    <button
                      className="flex items-center justify-center w-full min-h-[48px] px-4 py-3 bg-muted hover:bg-muted/80 active:bg-muted/60 rounded-lg transition-colors"
                      onClick={closeMobileSheet}
                    >
                      <X className="mr-2 h-4 w-4" />
                      <span className="text-sm font-medium">
                        {t('common:buttons.cancel')}
                      </span>
                    </button>
                  </div>
                </div>
              </motion.div>
            </>
          )}
        </AnimatePresence>,
          document.body
        )}
      </>
    );
  }

  // Desktop: Dropdown menu
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>{triggerButton}</DropdownMenuTrigger>
      <DropdownMenuContent align="end">
        {hasAttemptActions && (
          <>
            <DropdownMenuLabel>{t('actionsMenu.attempt')}</DropdownMenuLabel>
            <DropdownMenuItem
              disabled={!attempt?.id}
              onClick={handleOpenInEditor}
            >
              <ExternalLink className="mr-2 h-4 w-4" />
              {t('actionsMenu.openInIde')}
            </DropdownMenuItem>
            <DropdownMenuItem
              disabled={!attempt?.id}
              onClick={handleViewProcesses}
            >
              <Activity className="mr-2 h-4 w-4" />
              {t('actionsMenu.viewProcesses')}
            </DropdownMenuItem>
            <DropdownMenuItem
              disabled={isRemote}
              onClick={handleCreateNewAttempt}
              title={
                isRemote ? t('actionsMenu.remoteTaskCannotExecute') : undefined
              }
            >
              <Plus className="mr-2 h-4 w-4" />
              {t('actionsMenu.createNewAttempt')}
            </DropdownMenuItem>
            <DropdownMenuItem
              disabled={!attempt?.id || !task || isRemote}
              onClick={handleGitActions}
              title={
                isRemote ? t('actionsMenu.remoteTaskCannotExecute') : undefined
              }
            >
              <GitMerge className="mr-2 h-4 w-4" />
              {t('actionsMenu.gitActions')}
            </DropdownMenuItem>
            <DropdownMenuItem
              disabled={!attempt?.id || isRemote}
              onClick={handleEditBranchName}
              title={
                isRemote ? t('actionsMenu.remoteTaskCannotExecute') : undefined
              }
            >
              <Tag className="mr-2 h-4 w-4" />
              {t('actionsMenu.editBranchName')}
            </DropdownMenuItem>
            <DropdownMenuSeparator />
            <DropdownMenuItem
              disabled={
                !attempt?.id ||
                isRemote ||
                attempt.worktree_deleted ||
                purgeArtifacts.isPending
              }
              onClick={handlePurgeArtifacts}
              title={
                isRemote
                  ? t('actionsMenu.remoteTaskCannotExecute')
                  : attempt?.worktree_deleted
                    ? t(
                        'actionsMenu.worktreeAlreadyDeleted',
                        'Worktree already deleted'
                      )
                    : t(
                        'actionsMenu.purgeArtifactsDesc',
                        'Remove target/, node_modules/, etc.'
                      )
              }
            >
              <HardDrive className="mr-2 h-4 w-4" />
              {t('actionsMenu.purgeArtifacts', 'Purge Build Artifacts')}
            </DropdownMenuItem>
            <DropdownMenuItem
              disabled={
                !attempt?.id ||
                isRemote ||
                attempt.worktree_deleted ||
                cleanupWorktree.isPending
              }
              onClick={handleCleanupWorktree}
              className="text-destructive"
              title={
                isRemote
                  ? t('actionsMenu.remoteTaskCannotExecute')
                  : attempt?.worktree_deleted
                    ? t(
                        'actionsMenu.worktreeAlreadyDeleted',
                        'Worktree already deleted'
                      )
                    : t(
                        'actionsMenu.cleanupWorktreeDesc',
                        'Delete worktree files from disk'
                      )
              }
            >
              <FolderX className="mr-2 h-4 w-4" />
              {t('actionsMenu.cleanupWorktree', 'Delete Worktree')}
            </DropdownMenuItem>
            <DropdownMenuSeparator />
          </>
        )}

        {hasTaskActions && (
          <>
            <DropdownMenuLabel>{t('actionsMenu.task')}</DropdownMenuLabel>
            {/* Quick status change actions */}
            {task?.status !== 'inprogress' && (
              <DropdownMenuItem
                disabled={!canModifyTask || isUpdatingStatus}
                onClick={(e) => handleStatusChange(e, 'inprogress')}
              >
                <Play className="mr-2 h-4 w-4" />
                {t('actionsMenu.moveToInProgress')}
              </DropdownMenuItem>
            )}
            {task?.status !== 'inreview' && (
              <DropdownMenuItem
                disabled={!canModifyTask || isUpdatingStatus}
                onClick={(e) => handleStatusChange(e, 'inreview')}
              >
                <Eye className="mr-2 h-4 w-4" />
                {t('actionsMenu.moveToInReview')}
              </DropdownMenuItem>
            )}
            {task?.status !== 'done' && (
              <DropdownMenuItem
                disabled={!canModifyTask || isUpdatingStatus}
                onClick={(e) => handleStatusChange(e, 'done')}
              >
                <CheckCircle className="mr-2 h-4 w-4" />
                {t('actionsMenu.moveToDone')}
              </DropdownMenuItem>
            )}
            {task?.status !== 'cancelled' && (
              <DropdownMenuItem
                disabled={!canModifyTask || isUpdatingStatus}
                onClick={(e) => handleStatusChange(e, 'cancelled')}
              >
                <XCircle className="mr-2 h-4 w-4" />
                {t('actionsMenu.cancelTask')}
              </DropdownMenuItem>
            )}
            {task?.status !== 'todo' && (
              <DropdownMenuItem
                disabled={!canModifyTask || isUpdatingStatus}
                onClick={(e) => handleStatusChange(e, 'todo')}
              >
                <Circle className="mr-2 h-4 w-4" />
                {t('actionsMenu.moveToTodo')}
              </DropdownMenuItem>
            )}
            <DropdownMenuSeparator />
            <DropdownMenuItem
              disabled={!canReassign}
              onClick={handleReassign}
            >
              <UserPlus className="mr-2 h-4 w-4" />
              {t('actionsMenu.reassign')}
            </DropdownMenuItem>
            <DropdownMenuSeparator />
            <DropdownMenuItem
              disabled={!task?.id || !projectId}
              onClick={handleViewRelatedTasks}
            >
              <Link2 className="mr-2 h-4 w-4" />
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
              <GitBranch className="mr-2 h-4 w-4" />
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
                <Archive className="mr-2 h-4 w-4" />
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
                <ArchiveRestore className="mr-2 h-4 w-4" />
                {t('actionsMenu.unarchive')}
              </DropdownMenuItem>
            )}
            <DropdownMenuItem
              disabled={!projectId || !canModifyTask}
              onClick={handleEdit}
            >
              <Pencil className="mr-2 h-4 w-4" />
              {t('common:buttons.edit')}
            </DropdownMenuItem>
            <DropdownMenuItem disabled={!projectId} onClick={handleDuplicate}>
              <Copy className="mr-2 h-4 w-4" />
              {t('actionsMenu.duplicate')}
            </DropdownMenuItem>
            <DropdownMenuItem
              disabled={!projectId || !canModifyTask}
              onClick={handleDelete}
              className="text-destructive"
            >
              <Trash2 className="mr-2 h-4 w-4" />
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
              <UserPlus className="mr-2 h-4 w-4" />
              {t('actionsMenu.reassign')}
            </DropdownMenuItem>
          </>
        )}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
