import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
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

  // ── Local UI state ──────────────────────────────────────────────────────────

  const [workspaceName, setWorkspaceName] = useState('');
  const [hasAttemptedSubmit, setHasAttemptedSubmit] = useState(false);

  // mode is UI state — NOT persisted in CreateModeProvider draft.
  // Initialise from linkedIssue presence (set by kanban "create workspace from issue" flow).
  const [mode, setMode] = useState<CreateWorkspaceMode>(() =>
    linkedIssue != null ? 'link_task' : 'new_task'
  );

  // manualLinkedIssue: issue the user selects in LinkTaskRow.
  // Separate from the context-level linkedIssue (set by kanban entry point).
  const [manualLinkedIssue, setManualLinkedIssue] =
    useState<LinkedIssue | null>(() => linkedIssue ?? null);

  // When linkedIssue is set externally after mount (e.g. draft re-seed), sync mode.
  // Safe: setting mode/manualLinkedIssue does not affect linkedIssue.
  useEffect(() => {
    if (linkedIssue) {
      setMode('link_task');
      setManualLinkedIssue(linkedIssue);
    }
  }, [linkedIssue]);

  // ── Mode switching ──────────────────────────────────────────────────────────

  const handleModeChange = useCallback(
    (newMode: CreateWorkspaceMode) => {
      if (createWorkspace.isPending) return;
      if (newMode !== 'link_task') {
        if (linkedIssue) clearLinkedIssue();
        setManualLinkedIssue(null);
      }
      setMode(newMode);
    },
    [createWorkspace.isPending, linkedIssue, clearLinkedIssue]
  );

  const handleIssueSelect = useCallback((issue: LinkedIssue | null) => {
    setManualLinkedIssue(issue);
    if (!issue) setMode('new_task');
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

  // ── Submit ───────────────────────────────────────────────────────────────────

  const handleSubmit = useCallback(async () => {
    if (isSubmitting.current) return;
    isSubmitting.current = true;
    setHasAttemptedSubmit(true);

    if (!canSubmit || !executorConfig) {
      isSubmitting.current = false;
      return;
    }

    try {
      const { title: autoTitle } = splitMessageToTitleDescription(message);
      const data = {
        executor_config: executorConfig,
        name: workspaceName.trim() || autoTitle,
        prompt: message,
        repos: repos.map((r) => ({
          repo_id: r.id,
          target_branch: targetBranches[r.id]!,
        })),
        linked_issue: effectiveLinkedIssue
          ? {
              remote_project_id: effectiveLinkedIssue.remoteProjectId,
              issue_id: effectiveLinkedIssue.issueId,
            }
          : null,
        attachment_ids: getAttachmentIds(),
      };

      const linkToIssue = effectiveLinkedIssue
        ? {
            remoteProjectId: effectiveLinkedIssue.remoteProjectId,
            issueId: effectiveLinkedIssue.issueId,
          }
        : undefined;

      const result = await createWorkspace.mutateAsync({ data, linkToIssue });

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
        onWorkspaceCreated(result.workspace.id);
      }

      if (effectiveLinkedIssue?.remoteProjectId) {
        saveProjectRepoDefaults(
          effectiveLinkedIssue.remoteProjectId,
          data.repos
        ).catch((err) =>
          console.warn('Failed to save project repo defaults:', err)
        );
      }

      clearAttachments();
      await clearDraft();
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
    createWorkspace,
    onWorkspaceCreated,
    getAttachmentIds,
    clearAttachments,
    clearDraft,
    t,
  ]);

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
        : createWorkspace.error
          ? createWorkspace.error instanceof Error
            ? createWorkspace.error.message
            : 'Failed to create workspace'
          : null;

  // ── Guard: wait for draft initialisation ────────────────────────────────────

  if (!hasInitialValue) return null;

  const isBusy = createWorkspace.isPending;

  return (
    <div className="relative flex h-full flex-1 flex-col bg-primary overflow-y-auto">
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
