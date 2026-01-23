import {
  AlertDialog,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog';
import { Button } from '@/components/ui/button';

type Props = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  previousVariant: string;
  previousModel: string | null;
  newVariant: string;
  newModel: string | null;
  onConfirm: () => void;
};

function ModelChangeWarningDialog({
  open,
  onOpenChange,
  previousVariant,
  previousModel,
  newVariant,
  newModel,
  onConfirm,
}: Props) {
  const handleCancel = () => {
    onOpenChange(false);
  };

  const handleConfirm = () => {
    onConfirm();
    onOpenChange(false);
  };

  return (
    <AlertDialog open={open} onOpenChange={onOpenChange}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>Model Change Detected</AlertDialogTitle>
          <AlertDialogDescription>
            You are switching from{' '}
            <span className="font-semibold">
              {previousVariant}
              {previousModel && ` (${previousModel})`}
            </span>{' '}
            to{' '}
            <span className="font-semibold">
              {newVariant}
              {newModel && ` (${newModel})`}
            </span>
            .
            <br />
            <br />
            Switching between variants with different models will start a fresh
            session without prior context. The new executor will not have access
            to previous conversation history.
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <Button variant="outline" onClick={handleCancel}>
            Cancel
          </Button>
          <Button onClick={handleConfirm}>Continue Anyway</Button>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}

export default ModelChangeWarningDialog;
