import {
  AlertDialog,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog';
import { Button } from '@/components/ui/button';
import { useTranslation } from 'react-i18next';

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
  const { t } = useTranslation('tasks');

  const previousLabel = `${previousVariant}${
    previousModel ? ` (${previousModel})` : ''
  }`;
  const newLabel = `${newVariant}${newModel ? ` (${newModel})` : ''}`;

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
          <AlertDialogTitle>
            {t('modelChangeWarning.title')}
          </AlertDialogTitle>
          <AlertDialogDescription>
            {t('modelChangeWarning.summary', {
              previousLabel,
              newLabel,
            })}
            <br />
            <br />
            {t('modelChangeWarning.contextLoss')}
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <Button variant="outline" onClick={handleCancel}>
            {t('common:buttons.cancel')}
          </Button>
          <Button onClick={handleConfirm}>
            {t('modelChangeWarning.continueAnyway')}
          </Button>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}

export default ModelChangeWarningDialog;
