import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import NiceModal, { useModal } from '@ebay/nice-modal-react';
import { X } from 'lucide-react';
import { defineModal } from '@/lib/modals';
import TerminalContainer from '@/components/terminal/TerminalContainer';

export interface TerminalDialogProps {
  workingDir: string;
  title?: string;
}

const TerminalDialogImpl = NiceModal.create<TerminalDialogProps>((props) => {
  const modal = useModal();
  const { workingDir, title } = props;

  const handleClose = () => {
    modal.hide();
  };

  // Extract folder name from path for default title
  const folderName = workingDir.split('/').pop() || workingDir;
  const displayTitle = title || `Terminal - ${folderName}`;

  return (
    <Dialog open={modal.visible} onOpenChange={handleClose}>
      <DialogContent className="sm:max-w-[900px] h-[600px] flex flex-col p-0">
        <DialogHeader className="px-4 py-3 border-b flex-shrink-0">
          <div className="flex items-center justify-between">
            <DialogTitle className="text-sm font-medium">
              {displayTitle}
            </DialogTitle>
            <Button
              variant="ghost"
              size="icon"
              className="h-6 w-6"
              onClick={handleClose}
            >
              <X className="h-4 w-4" />
            </Button>
          </div>
        </DialogHeader>
        <div className="flex-1 min-h-0">
          <TerminalContainer
            workingDir={workingDir}
            className="h-full"
            onClose={handleClose}
          />
        </div>
      </DialogContent>
    </Dialog>
  );
});

export const TerminalDialog = defineModal<TerminalDialogProps, void>(
  TerminalDialogImpl
);
