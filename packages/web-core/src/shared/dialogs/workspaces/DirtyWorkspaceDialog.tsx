import { useState } from 'react';
import { create, useModal } from '@ebay/nice-modal-react';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@vibe/ui/components/KeyboardDialog';
import { Button } from '@vibe/ui/components/Button';
import { Checkbox } from '@vibe/ui/components/Checkbox';
import { Input } from '@vibe/ui/components/Input';
import { Label } from '@radix-ui/react-label';
import { WarningIcon } from '@phosphor-icons/react';
import { defineModal } from '@/shared/lib/modals';

export type DirtyWorkspaceOperation = 'archive' | 'delete';

export type DirtyWorkspaceDialogResult =
  | { action: 'cancel' }
  | { action: 'continue' }
  | {
      action: 'commit';
      message: string;
      push: boolean;
      createPr: boolean;
    };

interface DirtyWorkspaceDialogProps {
  operation: DirtyWorkspaceOperation;
  defaultCommitMessage: string;
}

const operationCopy = {
  archive: {
    title: 'Uncommitted Changes Detected',
    message:
      'This workspace has uncommitted changes. Archiving will remove its worktree from disk.',
    continueText: 'Archive Anyway',
    commitText: 'Commit Then Archive',
  },
  delete: {
    title: 'Uncommitted Changes Detected',
    message:
      'This workspace has uncommitted changes. Deleting will remove its worktree from disk.',
    continueText: 'Delete Anyway',
    commitText: 'Commit Then Delete',
  },
} satisfies Record<
  DirtyWorkspaceOperation,
  {
    title: string;
    message: string;
    continueText: string;
    commitText: string;
  }
>;

const DirtyWorkspaceDialogImpl = create<DirtyWorkspaceDialogProps>(
  ({ operation, defaultCommitMessage }) => {
    const modal = useModal();
    const copy = operationCopy[operation];
    const [message, setMessage] = useState(defaultCommitMessage);
    const [push, setPush] = useState(false);
    const [createPr, setCreatePr] = useState(false);

    const trimmedMessage = message.trim();

    const close = (result: DirtyWorkspaceDialogResult) => {
      modal.resolve(result);
      modal.hide();
    };

    return (
      <Dialog
        open={modal.visible}
        onOpenChange={(open) => !open && close({ action: 'cancel' })}
      >
        <DialogContent className="sm:max-w-[480px]">
          <DialogHeader>
            <div className="flex items-center gap-3">
              <WarningIcon className="h-6 w-6 text-destructive" />
              <DialogTitle>{copy.title}</DialogTitle>
            </div>
            <DialogDescription className="text-left pt-2">
              {copy.message}
            </DialogDescription>
          </DialogHeader>

          <div className="space-y-4">
            <div className="space-y-2">
              <Label htmlFor="dirty-workspace-commit-message">
                Commit message
              </Label>
              <Input
                id="dirty-workspace-commit-message"
                value={message}
                onChange={(event) => setMessage(event.target.value)}
                onCommandEnter={(event) => {
                  event.preventDefault();
                  if (!trimmedMessage) return;
                  close({
                    action: 'commit',
                    message: trimmedMessage,
                    push,
                    createPr,
                  });
                }}
              />
            </div>

            <label className="flex items-center gap-2 text-sm">
              <Checkbox checked={push} onCheckedChange={setPush} />
              Push committed changes
            </label>

            <label className="flex items-center gap-2 text-sm">
              <Checkbox checked={createPr} onCheckedChange={setCreatePr} />
              Create a pull request after committing
            </label>
          </div>

          <DialogFooter className="gap-2">
            <Button
              variant="outline"
              onClick={() => close({ action: 'cancel' })}
            >
              Cancel
            </Button>
            <Button
              variant="destructive"
              onClick={() => close({ action: 'continue' })}
            >
              {copy.continueText}
            </Button>
            <Button
              disabled={!trimmedMessage}
              onClick={() =>
                close({
                  action: 'commit',
                  message: trimmedMessage,
                  push,
                  createPr,
                })
              }
            >
              {copy.commitText}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    );
  }
);

export const DirtyWorkspaceDialog = defineModal<
  DirtyWorkspaceDialogProps,
  DirtyWorkspaceDialogResult
>(DirtyWorkspaceDialogImpl);
