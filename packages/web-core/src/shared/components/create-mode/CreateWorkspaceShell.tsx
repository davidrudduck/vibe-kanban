import {
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
} from 'react';
import { useTranslation } from 'react-i18next';
import { useDropzone } from 'react-dropzone';
import { useCreateMode } from '@/features/create-mode/model/useCreateMode';
import { AgentIcon } from '@/shared/components/AgentIcon';
import { useUserSystem } from '@/shared/hooks/useUserSystem';
import WYSIWYGEditor from '@/shared/components/WYSIWYGEditor';
import { useCreateWorkspace } from '@/shared/hooks/useCreateWorkspace';
import { useCreateAttachments } from '@/shared/hooks/useCreateAttachments';
import { useExecutorConfig } from '@/shared/hooks/useExecutorConfig';
import { saveProjectRepoDefaults } from '@/shared/hooks/useProjectRepoDefaults';
import { ProjectContext } from '@/shared/hooks/useProjectContext';
import { getSortedExecutorVariantKeys } from '@/shared/lib/executor';
import {
  toPrettyCase,
  splitMessageToTitleDescription,
} from '@/shared/lib/string';
import type { BaseCodingAgent } from 'shared/types';
import type {
  LinkedIssue,
  CreateWorkspaceMode,
} from '@/shared/types/createMode';
import { CreateChatBox } from '@vibe/ui/components/CreateChatBox';
import { ConfirmDialog } from '@vibe/ui/components/ConfirmDialog';
import { SettingsDialog } from '@/shared/dialogs/settings/SettingsDialog';
import { ModelSelectorContainer } from '@/shared/components/ModelSelectorContainer';
import { ModeTabsBar } from './ModeTabsBar';
import { RepoSelectorCards } from './RepoSelectorCards';
import { NewTaskRow } from './NewTaskRow';
import { LinkTaskRow } from './LinkTaskRow';
import { QuickRunRow } from './QuickRunRow';
import { OrphanIssueModal, type OrphanIssueState } from './OrphanIssueModal';

// Stable no-op for required callbacks that have no action in the shell layout.
// eslint-disable-next-line @typescript-eslint/no-empty-function
const noop = () => {};

interface CreateWorkspaceShellProps {
  onWorkspaceCreated: (workspaceId: string) => void;
}

export function CreateWorkspaceShell({
  onWorkspaceCreated,
}: CreateWorkspaceShellProps) {
  const { t } = useTranslation('common');
  const { profiles, config } = useUserSystem();

  const {
    repos,
    targetBranches,
    message,
    setMessage,
    clearDraft,
    hasInitialValue,
    linkedIssue,
    clearLinkedIssue,
    preferredExecutorConfig,
    executorConfig: draftConfig,
    setExecutorConfig: setDraftConfig,
    attachments: draftAttachments,
    setAttachments: setDraftAttachments,
  } = useCreateMode();

  const { createWorkspace, createDraftWorkspace } = useCreateWorkspace();
  const isSubmitting = useRef(false);
  // Track mount state to prevent setState on unmounted component.
  const isMountedRef = useRef(true);
  useEffect(() => {
    return () => {
      isMountedRef.current = false;
    };
  }, []);

  // ── Project context (nullable — only available inside kanban layout) ────────
  const projectContext = useContext(ProjectContext);

  // ── Local UI state ──────────────────────────────────────────────────────────

  const [workspaceName, setWorkspaceName] = useState('');
  const [hasAttemptedSubmit, setHasAttemptedSubmit] = useState(false);
  // Phase 2: orphan state when workspace creation fails after issue was created
  const [orphanedIssue, setOrphanedIssue] = useState<OrphanIssueState | null>(
    null
  );
  // True while insertIssue().persisted is in-flight — keeps UI locked during
  // the window between issue creation and workspace start.
  const [isCreatingIssue, setIsCreatingIssue] = useState(false);
  // Holds an error message if issue creation itself failed (separate from
  // createWorkspace.error which only covers the workspace mutation).
  const [issueCreateError, setIssueCreateError] = useState<string | null>(null);

  // mode is UI state — NOT persisted in CreateModeProvider draft.
  // Initialise from linkedIssue presence (set by kanban "create workspace from issue" flow).
  const [mode, setMode] = useState<CreateWorkspaceMode>(() =>
    linkedIssue != null ? 'link_task' : 'new_task'
  );

  // manualLinkedIssue: issue the user selects in LinkTaskRow.
  // Separate from the context-level linkedIssue (set by kanban entry point).
  const [manualLinkedIssue, setManualLinkedIssue] =
    useState<LinkedIssue | null>(() => linkedIssue ?? null);

  // Tracks whether linkedIssue has already been applied to local mode state.
  // Prevents the effect below from stomping a user's mode choice when
  // CreateModeProvider re-emits the same linkedIssue reference.
  const linkedIssueAppliedRef = useRef(linkedIssue != null);

  // Sync mode only when linkedIssue first becomes non-null after mount
  // (e.g. draft re-seed). Does NOT re-trigger once mode has been seeded
  // so user mode changes are preserved. The ref is reset at the call site
  // (handleModeChange) when linkedIssue is cleared, not here in the effect,
  // to avoid batching gaps.
  useEffect(() => {
    if (linkedIssue && !linkedIssueAppliedRef.current) {
      linkedIssueAppliedRef.current = true;
      setMode('link_task');
      setManualLinkedIssue(linkedIssue);
    }
  }, [linkedIssue]);

  // ── Mode switching ──────────────────────────────────────────────────────────

  const handleModeChange = useCallback(
    (newMode: CreateWorkspaceMode) => {
      if (createWorkspace.isPending) return;
      // Clear any orphan state when switching modes — the context has changed.
      if (orphanedIssue) setOrphanedIssue(null);
      if (newMode !== 'link_task') {
        if (linkedIssue) clearLinkedIssue();
        setManualLinkedIssue(null);
        // Reset the ref when we clear linkedIssue so a future seed can apply.
        // Doing this here (at the call site) instead of in the effect prevents
        // a synchronous-batching gap where rapid clear+set drops the new seed.
        linkedIssueAppliedRef.current = false;
      }
      setMode(newMode);
    },
    [createWorkspace.isPending, orphanedIssue, linkedIssue, clearLinkedIssue]
  );

  const handleIssueSelect = useCallback((issue: LinkedIssue | null) => {
    setManualLinkedIssue(issue);
    // Intentionally NOT switching back to new_task on deselect — the user
    // remains in link_task mode so they can search for a different issue
    // without losing the context of what they were trying to link.
  }, []);

  // ── Effective linked issue for submission ───────────────────────────────────

  const effectiveLinkedIssue =
    mode === 'link_task' ? (manualLinkedIssue ?? linkedIssue) : null;

  // ── Attachments ─────────────────────────────────────────────────────────────

  const handleInsertMarkdown = useCallback(
    (markdown: string) => {
      setMessage(message.trim() ? `${message}\n\n${markdown}` : markdown);
    },
    [message, setMessage]
  );

  const { uploadFiles, getAttachmentIds, clearAttachments, localAttachments } =
    useCreateAttachments(
      handleInsertMarkdown,
      draftAttachments,
      setDraftAttachments
    );

  const onDrop = useCallback(
    (acceptedFiles: File[]) => {
      if (acceptedFiles.length > 0) uploadFiles(acceptedFiles);
    },
    [uploadFiles]
  );

  const { getRootProps, getInputProps, isDragActive } = useDropzone({
    onDrop,
    disabled: createWorkspace.isPending,
    noClick: true,
    noKeyboard: true,
  });

  // ── Executor config ──────────────────────────────────────────────────────────

  const scratchConfig = useMemo(() => {
    if (!hasInitialValue) return undefined;
    return draftConfig ?? null;
  }, [hasInitialValue, draftConfig]);

  const {
    executorConfig,
    effectiveExecutor,
    selectedVariant,
    executorOptions,
    variantOptions,
    presetOptions,
    setOverrides: setExecutorOverrides,
  } = useExecutorConfig({
    profiles,
    lastUsedConfig: preferredExecutorConfig,
    scratchConfig,
    configExecutorProfile: config?.executor_profile,
    onPersist: (cfg) => setDraftConfig(cfg),
  });

  // ── Derived state ────────────────────────────────────────────────────────────

  // First visible (non-hidden) status column — used as target for New Task creation.
  const firstNonHiddenStatusId = useMemo(() => {
    const statuses = projectContext?.statuses ?? [];
    const visible = statuses
      .filter((s) => !s.hidden)
      .sort((a, b) => a.sort_order - b.sort_order);
    return visible[0]?.id ?? null;
  }, [projectContext?.statuses]);

  const hasSelectedRepos = repos.length > 0;
  const repoId = repos.length === 1 ? repos[0]?.id : undefined;

  const hasSelectedBranchesForAllRepos = repos.every(
    (repo) => !!targetBranches[repo.id]
  );

  const canSubmit =
    hasSelectedRepos &&
    hasSelectedBranchesForAllRepos &&
    message.trim().length > 0 &&
    effectiveExecutor !== null;

  // Summary strings for CreateChatBox (shown in the repo summary row).
  const repoSummaryLabel = useMemo(() => {
    if (repos.length === 0) return 'No repository selected';
    if (repos.length === 1) {
      const repo = repos[0]!;
      const branch = targetBranches[repo.id];
      const branchLabel = branch
        ? branch.length > 15
          ? `${branch.slice(0, 15)}…`
          : branch
        : 'Select branch';
      return `${repo.display_name || repo.name} · ${branchLabel}`;
    }
    return `${repos.length} repositories selected`;
  }, [repos, targetBranches]);

  const repoSummaryTitle = useMemo(
    () =>
      repos
        .map((repo) => {
          const branch = targetBranches[repo.id] ?? 'Select branch';
          return `${repo.display_name || repo.name} (${branch})`;
        })
        .join('\n'),
    [repos, targetBranches]
  );

  // ── Executor handlers ────────────────────────────────────────────────────────

  const handlePresetSelect = useCallback(
    (presetId: string | null) => {
      if (!effectiveExecutor) return;
      setDraftConfig({
        ...draftConfig,
        executor: effectiveExecutor,
        variant: presetId,
      });
    },
    [effectiveExecutor, draftConfig, setDraftConfig]
  );

  const handleCustomise = useCallback(() => {
    SettingsDialog.show({ initialSection: 'agents' });
  }, []);

  const handleExecutorChange = useCallback(
    (executor: BaseCodingAgent) => {
      const executorProfile = profiles?.[executor];
      if (!executorProfile) {
        setDraftConfig({ executor, variant: null });
        return;
      }
      const variants = getSortedExecutorVariantKeys(executorProfile);
      let targetVariant: string | null = null;
      if (
        config?.executor_profile?.executor === executor &&
        config?.executor_profile?.variant
      ) {
        const saved = config.executor_profile.variant;
        if (variants.includes(saved)) targetVariant = saved;
      }
      if (!targetVariant) {
        targetVariant = variants.includes('DEFAULT')
          ? 'DEFAULT'
          : (variants[0] ?? null);
      }
      setDraftConfig({ executor, variant: targetVariant });
    },
    [profiles, setDraftConfig, config?.executor_profile]
  );

  // ── Submit helpers ───────────────────────────────────────────────────────────

  /** Shared post-success cleanup: saves repo defaults, clears draft, navigates. */
  const handleWorkspaceSuccess = useCallback(
    async (
      workspaceId: string,
      repoInputs: Array<{ repo_id: string; target_branch: string }>,
      linkedIssueForSave: LinkedIssue | null
    ) => {
      if (linkedIssueForSave?.remoteProjectId) {
        saveProjectRepoDefaults(
          linkedIssueForSave.remoteProjectId,
          repoInputs
        ).catch((err) =>
          console.warn('Failed to save project repo defaults:', err)
        );
      }
      clearAttachments();
      // Clear draft BEFORE navigating — onWorkspaceCreated may unmount this
      // component immediately, leaving clearDraft unresolved if called after.
      await clearDraft();
      onWorkspaceCreated(workspaceId);
    },
    [onWorkspaceCreated, clearAttachments, clearDraft]
  );

  // ── Submit ───────────────────────────────────────────────────────────────────

  const handleSubmit = useCallback(async () => {
    // Use React Query's isPending as source of truth instead of a ref to prevent
    // race conditions where the ref is reset before the async mutation completes.
    if (createWorkspace.isPending) return;
    // Cmd+Enter must not start a new submission while the orphan modal is open.
    if (orphanedIssue) return;
    isSubmitting.current = true;
    setHasAttemptedSubmit(true);
    // Clear any stale errors from previous attempts.
    setIssueCreateError(null);
    createWorkspace.reset();

    if (!canSubmit || !executorConfig) {
      isSubmitting.current = false;
      return;
    }

    try {
      const { title: autoTitle } = splitMessageToTitleDescription(message);
      const repoInputs = repos.map((r) => ({
        repo_id: r.id,
        target_branch: targetBranches[r.id]!,
      }));

      // ── Phase 2: Issue-first creation for New Task mode ─────────────────────
      // When project context is available, create the kanban issue before the
      // workspace so the two are linked from the start.
      let workspaceLinkedIssue: LinkedIssue | null = effectiveLinkedIssue;
      let freshlyCreatedIssue: OrphanIssueState | null = null;

      if (mode === 'new_task' && projectContext && firstNonHiddenStatusId) {
        setIsCreatingIssue(true);
        try {
          const issueTitle =
            workspaceName.trim() || autoTitle || 'New workspace';
          const issuesInStatus = projectContext.issues.filter(
            (i) => i.status_id === firstNonHiddenStatusId
          );
          // Use reduce instead of Math.min(...array) to avoid call-stack
          // overflow on status columns with very large issue counts.
          const minSortOrder =
            issuesInStatus.length > 0
              ? issuesInStatus.reduce(
                  (min, i) => Math.min(min, i.sort_order),
                  Infinity
                )
              : 1000;

          const { persisted } = projectContext.insertIssue({
            project_id: projectContext.projectId,
            status_id: firstNonHiddenStatusId,
            title: issueTitle,
            description: message.trim() || null,
            priority: null,
            sort_order: minSortOrder - 1,
            start_date: null,
            target_date: null,
            completed_at: null,
            parent_issue_id: null,
            parent_issue_sort_order: null,
            extension_metadata: null,
          });

          const createdIssue = await persisted;
          if (!isMountedRef.current) return;
          freshlyCreatedIssue = {
            id: createdIssue.id,
            title: createdIssue.title,
            simpleId: createdIssue.simple_id,
            remoteProjectId: projectContext.projectId,
          };
          workspaceLinkedIssue = {
            issueId: freshlyCreatedIssue.id,
            simpleId: freshlyCreatedIssue.simpleId,
            title: freshlyCreatedIssue.title,
            remoteProjectId: freshlyCreatedIssue.remoteProjectId,
          };
        } catch (error) {
          // Issue creation failed — abort. No orphan risk since workspace was
          // never started. Log the error for debugging.
          console.error('Failed to create kanban card:', error);
          if (!isMountedRef.current) return;
          setIssueCreateError(
            'Failed to create kanban card. Please try again.'
          );
          return;
        } finally {
          setIsCreatingIssue(false);
        }
      }

      // ── Workspace creation ─────────────────────────────────────────────────
      const data = {
        executor_config: executorConfig,
        name: workspaceName.trim() || autoTitle,
        prompt: message,
        repos: repoInputs,
        linked_issue: workspaceLinkedIssue
          ? {
              remote_project_id: workspaceLinkedIssue.remoteProjectId,
              issue_id: workspaceLinkedIssue.issueId,
            }
          : null,
        attachment_ids: getAttachmentIds(),
      };

      const linkToIssue = workspaceLinkedIssue
        ? {
            remoteProjectId: workspaceLinkedIssue.remoteProjectId,
            issueId: workspaceLinkedIssue.issueId,
          }
        : undefined;

      let result;
      try {
        result = await createWorkspace.mutateAsync({ data, linkToIssue });
        if (!isMountedRef.current) return;
      } catch (error) {
        console.error('Workspace creation failed:', error);
        if (!isMountedRef.current) return;
        // Workspace failed. If we just created a fresh issue, surface the
        // orphan modal so the user can retry or remove the dangling card.
        if (freshlyCreatedIssue) {
          setOrphanedIssue(freshlyCreatedIssue);
        }
        return;
      }

      if (result.linkErrorMessage) {
        await ConfirmDialog.show({
          title: t('error'),
          message: t('workspaces.linkAfterCreateError', {
            defaultValue:
              'The workspace was created and started, but linking it to the issue failed. Error: {{error}}',
            error: result.linkErrorMessage,
          }),
          confirmText: t('ok'),
          showCancelButton: false,
        });
      }

      if (result.workspace) {
        await handleWorkspaceSuccess(
          result.workspace.id,
          repoInputs,
          workspaceLinkedIssue
        );
      }
    } finally {
      isSubmitting.current = false;
    }
  }, [
    canSubmit,
    executorConfig,
    message,
    workspaceName,
    repos,
    targetBranches,
    effectiveLinkedIssue,
    mode,
    orphanedIssue,
    projectContext,
    firstNonHiddenStatusId,
    createWorkspace,
    getAttachmentIds,
    handleWorkspaceSuccess,
    t,
  ]);

  // ── Orphan handlers ──────────────────────────────────────────────────────────

  const handleRetryWorkspace = useCallback(async () => {
    if (!orphanedIssue) return;
    // Use React Query's isPending to prevent double-click race conditions.
    if (createWorkspace.isPending) return;
    // Cannot build a valid retry payload without executor config.
    if (!executorConfig) return;
    isSubmitting.current = true;
    // Clear any stale error from a prior failed retry attempt.
    createWorkspace.reset();

    try {
      const { title: autoTitle } = splitMessageToTitleDescription(message);
      const repoInputs = repos.map((r) => ({
        repo_id: r.id,
        target_branch: targetBranches[r.id]!,
      }));
      const data = {
        executor_config: executorConfig,
        name: workspaceName.trim() || autoTitle,
        prompt: message,
        repos: repoInputs,
        linked_issue: {
          remote_project_id: orphanedIssue.remoteProjectId,
          issue_id: orphanedIssue.id,
        },
        attachment_ids: getAttachmentIds(),
      };
      const linkToIssue = {
        remoteProjectId: orphanedIssue.remoteProjectId,
        issueId: orphanedIssue.id,
      };

      const result = await createWorkspace.mutateAsync({ data, linkToIssue });
      if (!isMountedRef.current) return;
      if (result.workspace) {
        setOrphanedIssue(null);
        const linkedForSave: LinkedIssue = {
          issueId: orphanedIssue.id,
          simpleId: orphanedIssue.simpleId,
          title: orphanedIssue.title,
          remoteProjectId: orphanedIssue.remoteProjectId,
        };
        await handleWorkspaceSuccess(
          result.workspace.id,
          repoInputs,
          linkedForSave
        );
      }
    } catch (error) {
      console.error('Retry workspace creation failed:', error);
      // Retry failed — orphan modal stays visible for another attempt.
    } finally {
      isSubmitting.current = false;
    }
  }, [
    orphanedIssue,
    message,
    repos,
    targetBranches,
    executorConfig,
    workspaceName,
    createWorkspace,
    getAttachmentIds,
    handleWorkspaceSuccess,
  ]);

  const handleRemoveOrphanIssue = useCallback(() => {
    if (!orphanedIssue || !projectContext) return;
    projectContext.removeIssue(orphanedIssue.id);
    setOrphanedIssue(null);
  }, [orphanedIssue, projectContext]);

  // ── Save draft ───────────────────────────────────────────────────────────────

  const handleSaveDraft = useCallback(async () => {
    if (!hasSelectedRepos || !hasSelectedBranchesForAllRepos) {
      setHasAttemptedSubmit(true);
      return;
    }
    const { title: autoTitle } = splitMessageToTitleDescription(message);
    const name = workspaceName.trim() || autoTitle || null;
    try {
      const workspace = await createDraftWorkspace.mutateAsync({
        name,
        repos: repos.map((r) => ({
          repo_id: r.id,
          target_branch: targetBranches[r.id]!,
        })),
      });
      if (workspace) {
        await clearDraft();
        onWorkspaceCreated(workspace.id);
      }
    } catch {
      // error handled by mutation onError
    }
  }, [
    hasSelectedRepos,
    hasSelectedBranchesForAllRepos,
    message,
    workspaceName,
    repos,
    targetBranches,
    createDraftWorkspace,
    onWorkspaceCreated,
    clearDraft,
  ]);

  // ── Error display ─────────────────────────────────────────────────────────────

  const displayError =
    hasAttemptedSubmit && repos.length === 0
      ? 'Add at least one repository to create a workspace'
      : hasAttemptedSubmit && !hasSelectedBranchesForAllRepos
        ? 'Select a branch for every repository before creating a workspace'
        : issueCreateError
          ? issueCreateError
          : createWorkspace.error
            ? createWorkspace.error instanceof Error
              ? createWorkspace.error.message
              : 'Failed to create workspace'
            : null;

  // ── Guard: wait for draft initialisation ────────────────────────────────────

  if (!hasInitialValue) return null;

  const isBusy = createWorkspace.isPending || isCreatingIssue;

  return (
    <div className="relative flex h-full flex-1 flex-col bg-primary overflow-y-auto">
      {/* Orphan modal: shown when workspace creation failed after issue was created */}
      {orphanedIssue && (
        <OrphanIssueModal
          orphan={orphanedIssue}
          isRetrying={createWorkspace.isPending}
          onRetry={handleRetryWorkspace}
          onRemove={handleRemoveOrphanIssue}
        />
      )}
      <div className="flex flex-1 flex-col px-base py-base">
        <div className="mx-auto flex w-chat max-w-full flex-col gap-base">
          {/* Mode tabs */}
          <ModeTabsBar
            mode={mode}
            onChange={handleModeChange}
            disabled={isBusy}
          />

          {/* Repository section */}
          <div className="flex flex-col gap-half">
            <span className="text-xs font-medium uppercase tracking-wider text-low">
              Repository
            </span>
            <RepoSelectorCards disabled={isBusy} />
          </div>

          {/* Mode-specific row */}
          {mode === 'new_task' && (
            <NewTaskRow
              title={workspaceName}
              onTitleChange={setWorkspaceName}
              disabled={isBusy}
            />
          )}
          {mode === 'link_task' && (
            <LinkTaskRow
              selectedIssue={manualLinkedIssue}
              onIssueSelect={handleIssueSelect}
              disabled={isBusy}
            />
          )}
          {mode === 'quick_run' && <QuickRunRow />}

          {/* Chat box: editor + executor + submit */}
          <div className="flex justify-center @container">
            <CreateChatBox
              editor={{ value: message, onChange: setMessage }}
              renderEditor={({
                value,
                onChange,
                onCmdEnter,
                disabled,
                repoIds,
                repoId: editorRepoId,
                executor,
                onPasteFiles,
                localAttachments: editorAttachments,
              }) => (
                <WYSIWYGEditor
                  placeholder="Describe what you want to do…"
                  value={value}
                  onChange={onChange}
                  onCmdEnter={onCmdEnter}
                  disabled={disabled}
                  className="min-h-double max-h-[50vh] overflow-y-auto"
                  repoIds={repoIds}
                  repoId={editorRepoId}
                  executor={executor}
                  autoFocus
                  onPasteFiles={onPasteFiles}
                  localAttachments={editorAttachments}
                  sendShortcut={config?.send_message_shortcut}
                  rawMode={config?.input_editor_mode === 'RAW'}
                />
              )}
              agentIcon={
                <AgentIcon agent={effectiveExecutor} className="size-icon-xl" />
              }
              title={{ value: workspaceName, onChange: setWorkspaceName }}
              onSend={handleSubmit}
              isSending={isBusy}
              onSaveDraft={handleSaveDraft}
              isSavingDraft={createDraftWorkspace.isPending}
              disabled={!hasSelectedRepos}
              executor={{
                selected: effectiveExecutor,
                options: executorOptions,
                onChange: handleExecutorChange,
              }}
              formatExecutorLabel={toPrettyCase}
              error={displayError}
              repoIds={repos.map((r) => r.id)}
              repoId={repoId}
              modelSelector={
                effectiveExecutor ? (
                  <ModelSelectorContainer
                    agent={effectiveExecutor}
                    workspaceId={undefined}
                    onAdvancedSettings={handleCustomise}
                    presets={variantOptions}
                    selectedPreset={selectedVariant}
                    onPresetSelect={handlePresetSelect}
                    onOverrideChange={setExecutorOverrides}
                    executorConfig={executorConfig}
                    presetOptions={presetOptions}
                  />
                ) : undefined
              }
              onPasteFiles={uploadFiles}
              localAttachments={localAttachments}
              repoSummaryLabel={repoSummaryLabel}
              repoSummaryTitle={repoSummaryTitle}
              onEditRepos={noop}
              dropzone={{ getRootProps, getInputProps, isDragActive }}
            />
          </div>
        </div>
      </div>
    </div>
  );
}
