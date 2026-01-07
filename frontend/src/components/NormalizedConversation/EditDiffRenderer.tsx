import { useMemo } from 'react';
import {
  DiffView,
  DiffModeEnum,
  DiffLineType,
  parseInstance,
} from '@git-diff-view/react';
import { SquarePen, ExternalLink } from 'lucide-react';
import { useUserSystem } from '@/components/ConfigProvider';
import { getHighLightLanguageFromPath } from '@/utils/extToLanguage';
import { getActualTheme } from '@/utils/theme';
import '@/styles/diff-style-overrides.css';
import '@/styles/edit-diff-overrides.css';
import { cn } from '@/lib/utils';
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from '@/components/ui/tooltip';
import { FileViewDialog } from '@/components/dialogs';

type Props = {
  path: string;
  unifiedDiff: string;
  hasLineNumbers: boolean;
  expansionKey: string;
  defaultExpanded?: boolean;
  statusAppearance?: 'default' | 'denied' | 'timed_out';
  forceExpanded?: boolean;
};

/**
 * Process hunks for @git-diff-view/react
 * - Extract additions/deletions for display
 * - Decide whether to hide line numbers based on backend data
 */
function processUnifiedDiff(unifiedDiff: string, hasLineNumbers: boolean) {
  // Hide line numbers when backend says they are unreliable
  const hideNums = !hasLineNumbers;
  let isValidDiff;

  // Pre-compute additions/deletions using the library parser so counts are available while collapsed
  let additions = 0;
  let deletions = 0;
  try {
    const parsed = parseInstance.parse(unifiedDiff);
    for (const h of parsed.hunks) {
      for (const line of h.lines) {
        if (line.type === DiffLineType.Add) additions++;
        else if (line.type === DiffLineType.Delete) deletions++;
      }
    }
    isValidDiff = parsed.hunks.length > 0;
  } catch (err) {
    console.error('Failed to parse diff hunks:', err);
    isValidDiff = false;
  }

  return {
    hunks: [unifiedDiff],
    hideLineNumbers: hideNums,
    additions,
    deletions,
    isValidDiff,
  };
}

import { useExpandable } from '@/stores/useExpandableStore';

/**
 * Extract relative path within ~/.claude/ directory from a full path.
 * Returns null if path is not within ~/.claude/.
 *
 * Examples:
 * - "/home/user/.claude/plans/foo.md" -> "plans/foo.md"
 * - "/home/user/project/file.ts" -> null
 */
function getClaudeRelativePath(path: string): string | null {
  const match = path.match(/\.claude\/(.+)$/);
  return match ? match[1] : null;
}

function EditDiffRenderer({
  path,
  unifiedDiff,
  hasLineNumbers,
  expansionKey,
  defaultExpanded = false,
  statusAppearance = 'default',
  forceExpanded = false,
}: Props) {
  const { config } = useUserSystem();
  const [expanded, setExpanded] = useExpandable(expansionKey, defaultExpanded);
  const effectiveExpanded = forceExpanded || expanded;

  const theme = getActualTheme(config?.theme);
  const claudeRelativePath = getClaudeRelativePath(path);

  const handleViewFile = () => {
    if (claudeRelativePath) {
      void FileViewDialog.show({
        filePath: path,
        relativePath: claudeRelativePath,
      });
    }
  };
  const { hunks, hideLineNumbers, additions, deletions, isValidDiff } = useMemo(
    () => processUnifiedDiff(unifiedDiff, hasLineNumbers),
    [unifiedDiff, hasLineNumbers]
  );

  const hideLineNumbersClass = hideLineNumbers ? ' edit-diff-hide-nums' : '';

  const diffData = useMemo(() => {
    const lang = getHighLightLanguageFromPath(path) || 'plaintext';
    return {
      hunks,
      oldFile: { fileName: path, fileLang: lang },
      newFile: { fileName: path, fileLang: lang },
    };
  }, [hunks, path]);

  const headerClass = cn(
    'flex items-center gap-1.5 text-secondary-foreground',
    statusAppearance === 'denied' && 'text-red-700 dark:text-red-300',
    statusAppearance === 'timed_out' && 'text-amber-700 dark:text-amber-200'
  );

  return (
    <div>
      <div className={headerClass}>
        <SquarePen className="h-3 w-3 flex-shrink-0" />
        <span
          onClick={() => setExpanded()}
          className="text-sm font-mono overflow-x-auto cursor-pointer inline-flex items-center gap-1"
        >
          {path}{' '}
          <span style={{ color: 'hsl(var(--console-success))' }}>
            +{additions}
          </span>{' '}
          <span style={{ color: 'hsl(var(--console-error))' }}>
            -{deletions}
          </span>
          {/* View file link - inline after filename (only for ~/.claude/ paths) */}
          {claudeRelativePath && (
            <TooltipProvider>
              <Tooltip>
                <TooltipTrigger asChild>
                  <button
                    type="button"
                    onClick={(e) => {
                      e.stopPropagation();
                      handleViewFile();
                    }}
                    className="flex-shrink-0 text-muted-foreground hover:text-foreground transition-colors p-0.5"
                    aria-label="View file"
                  >
                    <ExternalLink className="h-3 w-3" />
                  </button>
                </TooltipTrigger>
                <TooltipContent>View file</TooltipContent>
              </Tooltip>
            </TooltipProvider>
          )}
        </span>
      </div>

      {effectiveExpanded && (
        <div className={'mt-2 border ' + hideLineNumbersClass}>
          {isValidDiff ? (
            <DiffView
              data={diffData}
              diffViewWrap={false}
              diffViewTheme={theme}
              diffViewHighlight
              diffViewMode={DiffModeEnum.Unified}
              diffViewFontSize={12}
            />
          ) : (
            <>
              <pre
                className="px-4 pb-4 text-xs font-mono overflow-x-auto whitespace-pre-wrap"
                style={{ color: 'hsl(var(--muted-foreground) / 0.9)' }}
              >
                {unifiedDiff}
              </pre>
            </>
          )}
        </div>
      )}
    </div>
  );
}

export default EditDiffRenderer;
