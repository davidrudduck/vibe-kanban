import { useMemo } from 'react';
import type { ProcessInfo } from 'shared/types';
import { ProcessCard } from './ProcessCard';
import { useTranslation } from 'react-i18next';
import { Loader2, Inbox } from 'lucide-react';

interface ProcessListProps {
  processes: ProcessInfo[];
  isLoading?: boolean;
  onKillProcess?: (pid: number) => void;
  killingPids?: Set<number>;
}

interface ProcessNode {
  process: ProcessInfo;
  children: ProcessNode[];
}

/**
 * Build a tree structure from flat process list based on parent_pid relationships.
 * Roots are processes whose parent is not in the list.
 */
function buildProcessTree(processes: ProcessInfo[]): ProcessNode[] {
  const pidSet = new Set(processes.map((p) => p.pid));
  const childrenMap = new Map<number, ProcessInfo[]>();

  // Group children by parent PID
  for (const process of processes) {
    if (process.parent_pid !== null && pidSet.has(process.parent_pid)) {
      const siblings = childrenMap.get(process.parent_pid) || [];
      siblings.push(process);
      childrenMap.set(process.parent_pid, siblings);
    }
  }

  // Recursively build nodes
  function buildNode(process: ProcessInfo): ProcessNode {
    const children = childrenMap.get(process.pid) || [];
    return {
      process,
      children: children.map(buildNode),
    };
  }

  // Find root processes (no parent in our list)
  const roots = processes.filter(
    (p) => p.parent_pid === null || !pidSet.has(p.parent_pid)
  );

  // Sort roots: executors first, then by PID
  roots.sort((a, b) => {
    if (a.is_executor && !b.is_executor) return -1;
    if (!a.is_executor && b.is_executor) return 1;
    return a.pid - b.pid;
  });

  return roots.map(buildNode);
}

interface ProcessNodeRendererProps {
  node: ProcessNode;
  depth: number;
  onKillProcess?: (pid: number) => void;
  killingPids?: Set<number>;
}

function ProcessNodeRenderer({
  node,
  depth,
  onKillProcess,
  killingPids,
}: ProcessNodeRendererProps) {
  const isKilling = killingPids?.has(node.process.pid);

  return (
    <div className="space-y-2">
      <ProcessCard
        process={node.process}
        onKill={onKillProcess}
        isKilling={isKilling}
        isChild={depth > 0}
      />
      {node.children.length > 0 && (
        <div className="space-y-2 ml-4 pl-2 border-l border-muted">
          {node.children.map((child) => (
            <ProcessNodeRenderer
              key={child.process.pid}
              node={child}
              depth={depth + 1}
              onKillProcess={onKillProcess}
              killingPids={killingPids}
            />
          ))}
        </div>
      )}
    </div>
  );
}

export function ProcessList({
  processes,
  isLoading,
  onKillProcess,
  killingPids,
}: ProcessListProps) {
  const { t } = useTranslation('processes');

  const processTree = useMemo(() => buildProcessTree(processes), [processes]);

  if (isLoading) {
    return (
      <div className="flex flex-col items-center justify-center py-12 text-muted-foreground">
        <Loader2 className="h-8 w-8 animate-spin mb-2" />
        <p>{t('loading')}</p>
      </div>
    );
  }

  if (processes.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center py-12 text-muted-foreground">
        <Inbox className="h-12 w-12 mb-4" />
        <p className="text-lg font-medium">{t('empty.title')}</p>
        <p className="text-sm">{t('empty.description')}</p>
      </div>
    );
  }

  return (
    <div className="space-y-3">
      {processTree.map((node) => (
        <ProcessNodeRenderer
          key={node.process.pid}
          node={node}
          depth={0}
          onKillProcess={onKillProcess}
          killingPids={killingPids}
        />
      ))}
    </div>
  );
}

export default ProcessList;
