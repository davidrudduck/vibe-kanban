import { useState } from 'react';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Label } from '@/components/ui/label';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import { Loader2, AlertTriangle, GitMerge } from 'lucide-react';
import { useNodeMutations } from '@/hooks/useNodeMutations';
import type { Node } from '@/types/nodes';

interface MergeNodesDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  sourceNode: Node;
  availableTargets: Node[];
  onSuccess?: () => void;
}

export function MergeNodesDialog({
  open,
  onOpenChange,
  sourceNode,
  availableTargets,
  onSuccess,
}: MergeNodesDialogProps) {
  const [targetNodeId, setTargetNodeId] = useState<string>('');
  const [error, setError] = useState<string | null>(null);

  const { mergeNodes } = useNodeMutations({
    onMergeSuccess: (result) => {
      onOpenChange(false);
      setTargetNodeId('');
      setError(null);
      onSuccess?.();
      console.log(
        `Merged ${result.projects_moved} projects and ${result.keys_rebound} keys`
      );
    },
    onMergeError: (err) => {
      setError(err instanceof Error ? err.message : 'Failed to merge nodes');
    },
  });

  const handleMerge = () => {
    if (!targetNodeId) return;
    setError(null);
    mergeNodes.mutate({
      sourceNodeId: sourceNode.id,
      targetNodeId,
    });
  };

  const handleClose = () => {
    if (mergeNodes.isPending) return;
    onOpenChange(false);
    setTargetNodeId('');
    setError(null);
  };

  const targetNode = availableTargets.find((n) => n.id === targetNodeId);

  return (
    <Dialog open={open} onOpenChange={handleClose}>
      <DialogContent className="sm:max-w-[500px]">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <GitMerge className="h-5 w-5" />
            Merge Node
          </DialogTitle>
          <DialogDescription>
            Merge &quot;{sourceNode.name}&quot; into another node. All projects
            and API keys will be transferred to the target node.
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-4 py-4">
          <Alert variant="destructive">
            <AlertTriangle className="h-4 w-4" />
            <AlertTitle>Warning</AlertTitle>
            <AlertDescription>
              This action is irreversible. The source node &quot;
              {sourceNode.name}&quot; will be deleted after merging.
            </AlertDescription>
          </Alert>

          {error && (
            <Alert variant="destructive">
              <AlertDescription>{error}</AlertDescription>
            </Alert>
          )}

          <div className="space-y-2">
            <Label htmlFor="target-node">Target Node</Label>
            <Select value={targetNodeId} onValueChange={setTargetNodeId}>
              <SelectTrigger id="target-node">
                <SelectValue placeholder="Select a node to merge into" />
              </SelectTrigger>
              <SelectContent>
                {availableTargets.length === 0 ? (
                  <SelectItem value="_none" disabled>
                    No other nodes available
                  </SelectItem>
                ) : (
                  availableTargets.map((node) => (
                    <SelectItem key={node.id} value={node.id}>
                      {node.name} ({node.status})
                    </SelectItem>
                  ))
                )}
              </SelectContent>
            </Select>
            {targetNode && (
              <p className="text-xs text-muted-foreground">
                Projects and API keys from &quot;{sourceNode.name}&quot; will be
                moved to &quot;{targetNode.name}&quot;
              </p>
            )}
          </div>
        </div>

        <DialogFooter>
          <Button
            variant="outline"
            onClick={handleClose}
            disabled={mergeNodes.isPending}
          >
            Cancel
          </Button>
          <Button
            variant="destructive"
            onClick={handleMerge}
            disabled={!targetNodeId || mergeNodes.isPending}
          >
            {mergeNodes.isPending ? (
              <>
                <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                Merging...
              </>
            ) : (
              <>
                <GitMerge className="h-4 w-4 mr-2" />
                Merge Node
              </>
            )}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
