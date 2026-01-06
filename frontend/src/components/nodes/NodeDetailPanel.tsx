import { useState } from 'react';
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Skeleton } from '@/components/ui/skeleton';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { NodeStatusBadge } from './NodeStatusBadge';
import { NodeProjectsList } from './NodeProjectsList';
import { MergeNodesDialog } from '@/components/dialogs/nodes/MergeNodesDialog';
import { useNode } from '@/hooks/useNode';
import { useNodeMutations } from '@/hooks/useNodeMutations';
import {
  Server,
  Cpu,
  Clock,
  Globe,
  Trash2,
  GitMerge,
  AlertCircle,
  Loader2,
} from 'lucide-react';
import { formatDistanceToNow, format } from 'date-fns';
import type { Node } from '@/types/nodes';

interface NodeDetailPanelProps {
  nodeId: string;
  availableNodes?: Node[];
  onDelete?: () => void;
  onMerge?: () => void;
}

export function NodeDetailPanel({
  nodeId,
  availableNodes = [],
  onDelete,
  onMerge,
}: NodeDetailPanelProps) {
  const [showMergeDialog, setShowMergeDialog] = useState(false);
  const [deleteError, setDeleteError] = useState<string | null>(null);

  const { data: node, isLoading, isError, error } = useNode(nodeId);

  const { deleteNode } = useNodeMutations({
    onDeleteSuccess: () => {
      onDelete?.();
    },
    onDeleteError: (err) => {
      setDeleteError(
        err instanceof Error ? err.message : 'Failed to delete node'
      );
    },
  });

  const handleDelete = () => {
    if (
      !confirm(
        `Are you sure you want to delete node "${node?.name}"? This action cannot be undone.`
      )
    ) {
      return;
    }
    setDeleteError(null);
    deleteNode.mutate(nodeId);
  };

  const handleMergeSuccess = () => {
    onMerge?.();
  };

  // Filter out current node from available targets
  const mergeTargets = availableNodes.filter((n) => n.id !== nodeId);

  if (isLoading) {
    return (
      <div className="space-y-6">
        <Card>
          <CardHeader>
            <div className="flex items-center gap-3">
              <Skeleton className="h-8 w-8 rounded" />
              <div className="space-y-2">
                <Skeleton className="h-6 w-48" />
                <Skeleton className="h-4 w-32" />
              </div>
            </div>
          </CardHeader>
          <CardContent>
            <div className="grid gap-4 md:grid-cols-2">
              {[1, 2, 3, 4].map((i) => (
                <div key={i} className="space-y-2">
                  <Skeleton className="h-4 w-24" />
                  <Skeleton className="h-5 w-36" />
                </div>
              ))}
            </div>
          </CardContent>
        </Card>
        <Card>
          <CardHeader>
            <Skeleton className="h-6 w-32" />
          </CardHeader>
          <CardContent>
            <div className="space-y-3">
              {[1, 2].map((i) => (
                <Skeleton key={i} className="h-16 w-full" />
              ))}
            </div>
          </CardContent>
        </Card>
      </div>
    );
  }

  if (isError || !node) {
    return (
      <Alert variant="destructive">
        <AlertCircle className="h-4 w-4" />
        <AlertDescription>
          Failed to load node: {error?.message || 'Unknown error'}
        </AlertDescription>
      </Alert>
    );
  }

  const lastSeen = node.last_heartbeat_at
    ? formatDistanceToNow(new Date(node.last_heartbeat_at), { addSuffix: true })
    : 'Never';

  const connectedAt = node.connected_at
    ? format(new Date(node.connected_at), 'PPpp')
    : null;

  const disconnectedAt = node.disconnected_at
    ? format(new Date(node.disconnected_at), 'PPpp')
    : null;

  const createdAt = format(new Date(node.created_at), 'PPpp');

  return (
    <div className="space-y-6">
      {/* Node Info Card */}
      <Card>
        <CardHeader>
          <div className="flex flex-col sm:flex-row sm:items-start sm:justify-between gap-4">
            <div className="flex items-center gap-3">
              <div className="p-2 bg-secondary rounded-lg">
                <Server className="h-6 w-6" />
              </div>
              <div>
                <CardTitle className="text-xl">{node.name}</CardTitle>
                <CardDescription className="flex items-center gap-2 mt-1">
                  <Clock className="h-3 w-3" />
                  Last seen {lastSeen}
                </CardDescription>
              </div>
            </div>
            <NodeStatusBadge status={node.status} />
          </div>
        </CardHeader>
        <CardContent>
          {deleteError && (
            <Alert variant="destructive" className="mb-4">
              <AlertDescription>{deleteError}</AlertDescription>
            </Alert>
          )}

          <div className="grid gap-4 md:grid-cols-2">
            <div>
              <div className="text-sm font-medium text-muted-foreground">
                Operating System
              </div>
              <div className="flex items-center gap-2 mt-1">
                <Cpu className="h-4 w-4 text-muted-foreground" />
                <span>
                  {node.capabilities.os || 'Unknown'} /{' '}
                  {node.capabilities.arch || 'Unknown'}
                </span>
              </div>
            </div>

            <div>
              <div className="text-sm font-medium text-muted-foreground">
                Version
              </div>
              <div className="mt-1 font-mono text-sm">
                {node.capabilities.version || 'Unknown'}
                {node.capabilities.git_commit && (
                  <span className="text-muted-foreground ml-2">
                    ({node.capabilities.git_commit}
                    {node.capabilities.git_branch &&
                      ` @ ${node.capabilities.git_branch}`}
                    )
                  </span>
                )}
              </div>
            </div>

            {node.public_url && (
              <div className="md:col-span-2">
                <div className="text-sm font-medium text-muted-foreground">
                  Public URL
                </div>
                <div className="flex items-center gap-2 mt-1">
                  <Globe className="h-4 w-4 text-muted-foreground" />
                  <a
                    href={node.public_url}
                    target="_blank"
                    rel="noopener noreferrer"
                    className="text-primary hover:underline truncate"
                  >
                    {node.public_url}
                  </a>
                </div>
              </div>
            )}

            <div>
              <div className="text-sm font-medium text-muted-foreground">
                Max Concurrent Tasks
              </div>
              <div className="mt-1">
                {node.capabilities.max_concurrent_tasks}
              </div>
            </div>

            <div>
              <div className="text-sm font-medium text-muted-foreground">
                Created
              </div>
              <div className="mt-1 text-sm">{createdAt}</div>
            </div>

            {connectedAt && (
              <div>
                <div className="text-sm font-medium text-muted-foreground">
                  Connected
                </div>
                <div className="mt-1 text-sm">{connectedAt}</div>
              </div>
            )}

            {disconnectedAt && node.status === 'offline' && (
              <div>
                <div className="text-sm font-medium text-muted-foreground">
                  Disconnected
                </div>
                <div className="mt-1 text-sm">{disconnectedAt}</div>
              </div>
            )}
          </div>

          {/* Executors */}
          {node.capabilities.executors.length > 0 && (
            <div className="mt-4">
              <div className="text-sm font-medium text-muted-foreground mb-2">
                Available Executors
              </div>
              <div className="flex flex-wrap gap-2">
                {node.capabilities.executors.map((executor) => (
                  <Badge key={executor} variant="secondary">
                    {executor}
                  </Badge>
                ))}
              </div>
            </div>
          )}

          {/* Actions */}
          <div className="flex flex-wrap gap-2 mt-6 pt-4 border-t">
            {mergeTargets.length > 0 && (
              <Button
                variant="outline"
                size="sm"
                onClick={() => setShowMergeDialog(true)}
              >
                <GitMerge className="h-4 w-4 mr-2" />
                Merge into Another Node
              </Button>
            )}
            <Button
              variant="destructive"
              size="sm"
              onClick={handleDelete}
              disabled={deleteNode.isPending}
            >
              {deleteNode.isPending ? (
                <Loader2 className="h-4 w-4 mr-2 animate-spin" />
              ) : (
                <Trash2 className="h-4 w-4 mr-2" />
              )}
              Delete Node
            </Button>
          </div>
        </CardContent>
      </Card>

      {/* Projects List */}
      <NodeProjectsList nodeId={nodeId} />

      {/* Merge Dialog */}
      <MergeNodesDialog
        open={showMergeDialog}
        onOpenChange={setShowMergeDialog}
        sourceNode={node}
        availableTargets={mergeTargets}
        onSuccess={handleMergeSuccess}
      />
    </div>
  );
}
