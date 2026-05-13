import { useCallback, useMemo, useState } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import {
  FolderOpenIcon,
  GitBranchIcon,
  PlusIcon,
  SpinnerIcon,
  XIcon,
} from '@phosphor-icons/react';
import { useTranslation } from 'react-i18next';
import type { Repo } from 'shared/types';
import type { BranchItem } from '@/shared/types/selectionItems';
import { repoApi } from '@/shared/lib/api';
import { cn } from '@/shared/lib/utils';
import { useCreateMode } from '@/features/create-mode/model/useCreateMode';
import { FolderPickerDialog } from '@/shared/dialogs/shared/FolderPickerDialog';
import {
  SelectionDialog,
  type SelectionPage,
} from '@/shared/dialogs/command-bar/SelectionDialog';
import {
  buildBranchSelectionPages,
  type BranchSelectionResult,
} from '@/shared/dialogs/command-bar/selections/branchSelection';
import { SettingsDialog } from '@/shared/dialogs/settings/SettingsDialog';

const VISIBLE_CARD_COUNT = 4;

function getRepoDisplayName(repo: Repo): string {
  return repo.display_name || repo.name;
}

function truncatePath(path: string, maxLen = 32): string {
  if (path.length <= maxLen) return path;
  const start = path.slice(0, 10);
  const end = path.slice(-(maxLen - 13));
  return `${start}…${end}`;
}

function toBranchItem(branch: {
  name: string;
  is_current: boolean;
}): BranchItem {
  return { name: branch.name, isCurrent: branch.is_current };
}

interface RepoSelectorCardsProps {
  disabled?: boolean;
}

export function RepoSelectorCards({ disabled }: RepoSelectorCardsProps) {
  const { t } = useTranslation('common');
  const queryClient = useQueryClient();
  const { repos, addRepo, removeRepo, targetBranches, setTargetBranch } =
    useCreateMode();

  const [showAll, setShowAll] = useState(false);
  const [pendingRepoId, setPendingRepoId] = useState<string | null>(null);
  const [isBrowsing, setIsBrowsing] = useState(false);
  const [pickerError, setPickerError] = useState<string | null>(null);

  const {
    data: allRepos,
    isLoading: isReposLoading,
    isError,
    error,
  } = useQuery({
    queryKey: ['repos', 'recent'],
    queryFn: () => repoApi.listRecent(),
  });

  const selectedRepoIds = useMemo(
    () => new Set(repos.map((r) => r.id)),
    [repos]
  );

  const visibleRepos = useMemo(() => {
    if (!allRepos) return [];
    if (showAll || allRepos.length <= VISIBLE_CARD_COUNT) return allRepos;
    return allRepos.slice(0, VISIBLE_CARD_COUNT);
  }, [allRepos, showAll]);

  const hiddenCount = Math.max(0, (allRepos?.length ?? 0) - VISIBLE_CARD_COUNT);

  const pickBranchForRepo = useCallback(async (repo: Repo) => {
    const branches = await repoApi.getBranches(repo.id);
    const branchItems = branches.map(toBranchItem);
    const result = (await SelectionDialog.show({
      initialPageId: 'selectBranch',
      pages: buildBranchSelectionPages(
        branchItems,
        getRepoDisplayName(repo)
      ) as Record<string, SelectionPage>,
    })) as BranchSelectionResult | undefined;
    return result?.branch ?? null;
  }, []);

  const handleCardClick = useCallback(
    async (repo: Repo) => {
      if (disabled || pendingRepoId === repo.id) return;
      setPickerError(null);

      if (selectedRepoIds.has(repo.id)) {
        removeRepo(repo.id);
        return;
      }

      setPendingRepoId(repo.id);
      try {
        const branch = await pickBranchForRepo(repo);
        if (!branch) return;
        addRepo(repo);
        setTargetBranch(repo.id, branch);
      } catch {
        setPickerError('Failed to load branches');
      } finally {
        setPendingRepoId(null);
      }
    },
    [
      disabled,
      pendingRepoId,
      selectedRepoIds,
      removeRepo,
      pickBranchForRepo,
      addRepo,
      setTargetBranch,
    ]
  );

  const handleChangeBranch = useCallback(
    async (e: React.MouseEvent, repo: Repo) => {
      e.stopPropagation();
      if (disabled || pendingRepoId !== null) return;
      setPendingRepoId(repo.id);
      setPickerError(null);
      try {
        const branch = await pickBranchForRepo(repo);
        if (branch) setTargetBranch(repo.id, branch);
      } catch {
        setPickerError('Failed to load branches');
      } finally {
        setPendingRepoId(null);
      }
    },
    [disabled, pendingRepoId, pickBranchForRepo, setTargetBranch]
  );

  const handleRemoveRepo = useCallback(
    (e: React.MouseEvent, repoId: string) => {
      e.stopPropagation();
      removeRepo(repoId);
    },
    [removeRepo]
  );

  const handleBrowse = useCallback(async () => {
    if (disabled || isBrowsing) return;
    setIsBrowsing(true);
    setPickerError(null);
    try {
      const selectedPath = await FolderPickerDialog.show({
        title: t('dialogs.selectGitRepository'),
        description: t('dialogs.chooseExistingRepo'),
      });
      if (!selectedPath) return;
      const repo = await repoApi.register({ path: selectedPath });
      queryClient.invalidateQueries({ queryKey: ['repos'] });
      queryClient.invalidateQueries({ queryKey: ['repos', 'recent'] });
      const branch = await pickBranchForRepo(repo);
      if (!branch) return;
      addRepo(repo);
      setTargetBranch(repo.id, branch);
    } catch {
      setPickerError('Failed to register repository');
    } finally {
      setIsBrowsing(false);
    }
  }, [
    disabled,
    isBrowsing,
    t,
    queryClient,
    pickBranchForRepo,
    addRepo,
    setTargetBranch,
  ]);

  if (isReposLoading) {
    return (
      <div className="flex items-center gap-half py-half text-sm text-low">
        <SpinnerIcon className="size-icon-xs animate-spin" />
        <span>Loading repositories…</span>
      </div>
    );
  }

  if (isError) {
    return (
      <div className="flex flex-col gap-half">
        <p className="text-sm text-error">
          Failed to load repositories:{' '}
          {error instanceof Error ? error.message : 'Unknown error'}
        </p>
        <button
          type="button"
          onClick={handleBrowse}
          disabled={isBrowsing}
          className="flex w-fit items-center gap-1 text-sm text-low hover:text-normal disabled:opacity-50"
        >
          {isBrowsing ? (
            <SpinnerIcon className="size-icon-xs animate-spin" />
          ) : (
            <FolderOpenIcon className="size-icon-xs" weight="bold" />
          )}
          <span>Browse for a folder</span>
        </button>
      </div>
    );
  }

  if (!allRepos || allRepos.length === 0) {
    return (
      <div className="flex flex-col gap-half">
        <p className="text-sm text-low">No repositories configured yet.</p>
        <button
          type="button"
          onClick={() => SettingsDialog.show({ initialSection: 'repos' })}
          className="w-fit text-sm font-medium text-brand underline hover:text-brand/80"
        >
          Add a repository in Settings
        </button>
        <button
          type="button"
          onClick={handleBrowse}
          disabled={isBrowsing}
          className="flex w-fit items-center gap-1 text-sm text-low hover:text-normal disabled:opacity-50"
        >
          {isBrowsing ? (
            <SpinnerIcon className="size-icon-xs animate-spin" />
          ) : (
            <FolderOpenIcon className="size-icon-xs" weight="bold" />
          )}
          <span>Browse for a folder</span>
        </button>
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-half">
      <div className="flex flex-wrap gap-half">
        {visibleRepos.map((repo) => {
          const isSelected = selectedRepoIds.has(repo.id);
          const branch = targetBranches[repo.id];
          const isPending = pendingRepoId === repo.id;
          const displayName = getRepoDisplayName(repo);
          // HTML spec forbids interactive elements inside <button>.
          // Use a div[role="button"] so inner Remove/Branch buttons are valid.
          const isCardDisabled =
            disabled || (pendingRepoId !== null && !isPending);

          return (
            <div
              key={repo.id}
              role="button"
              tabIndex={isCardDisabled ? -1 : 0}
              aria-pressed={isSelected}
              aria-disabled={isCardDisabled}
              title={`${displayName} — ${repo.path}`}
              onClick={() => handleCardClick(repo)}
              onKeyDown={(e) => {
                if (e.key === 'Enter' || e.key === ' ') {
                  e.preventDefault();
                  handleCardClick(repo);
                }
              }}
              className={cn(
                'relative flex min-w-[140px] max-w-[200px] flex-col gap-quarter rounded-sm border p-half text-left',
                'cursor-pointer transition-colors',
                isCardDisabled && 'cursor-not-allowed opacity-50',
                isSelected
                  ? 'border-brand/60 bg-brand/5 text-high ring-1 ring-brand/30'
                  : 'border-border/60 bg-secondary text-normal hover:border-border hover:text-high'
              )}
            >
              {isPending && (
                <div className="absolute inset-0 flex items-center justify-center rounded-sm bg-primary/60">
                  <SpinnerIcon className="size-icon-xs animate-spin text-brand" />
                </div>
              )}

              <div className="flex min-w-0 items-start justify-between gap-half">
                <span className="min-w-0 truncate text-sm font-medium leading-tight">
                  {displayName}
                </span>
                {isSelected && (
                  <button
                    type="button"
                    onClick={(e) => handleRemoveRepo(e, repo.id)}
                    disabled={disabled || pendingRepoId !== null}
                    aria-label={`Remove ${displayName}`}
                    className="shrink-0 text-low hover:text-error disabled:opacity-50"
                  >
                    <XIcon className="size-icon-2xs" weight="bold" />
                  </button>
                )}
              </div>

              <span className="truncate font-mono text-xs text-low">
                {truncatePath(repo.path)}
              </span>

              {isSelected && (
                <button
                  type="button"
                  onClick={(e) => handleChangeBranch(e, repo)}
                  disabled={disabled || pendingRepoId !== null}
                  className="flex items-center gap-quarter text-xs text-brand hover:text-brand/80 disabled:opacity-50"
                  aria-label={`Change branch for ${displayName}`}
                >
                  <GitBranchIcon
                    className="size-icon-2xs shrink-0"
                    weight="bold"
                  />
                  <span className="max-w-[120px] truncate">
                    {branch ?? 'Select branch'}
                  </span>
                </button>
              )}
            </div>
          );
        })}

        {/* Overflow toggle */}
        {!showAll && hiddenCount > 0 && (
          <button
            type="button"
            onClick={() => setShowAll(true)}
            className={cn(
              'flex min-w-[80px] items-center justify-center rounded-sm border border-border/60',
              'bg-secondary px-half py-half text-sm text-low hover:text-normal'
            )}
          >
            +{hiddenCount} more
          </button>
        )}
      </div>

      {/* Custom folder option */}
      <button
        type="button"
        onClick={handleBrowse}
        disabled={disabled || isBrowsing}
        className="flex w-fit items-center gap-1 text-xs text-low hover:text-normal disabled:opacity-50"
      >
        {isBrowsing ? (
          <SpinnerIcon className="size-icon-2xs animate-spin" />
        ) : (
          <PlusIcon className="size-icon-2xs" weight="bold" />
        )}
        <span>Custom folder</span>
      </button>

      {pickerError && <p className="text-xs text-error">{pickerError}</p>}
    </div>
  );
}
