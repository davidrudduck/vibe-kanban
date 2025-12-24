import { useState, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Alert } from '@/components/ui/alert';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from '@/components/ui/dialog';
import { Loader2 } from 'lucide-react';
import { labelsApi } from '@/lib/api';
import { ColorPicker, IconPicker, LabelBadge } from '@/components/labels';
import type {
  Label as LabelType,
  CreateLabel,
  UpdateLabel,
} from 'shared/types';
import NiceModal, { useModal } from '@ebay/nice-modal-react';
import { defineModal, getErrorMessage } from '@/lib/modals';

export interface LabelEditDialogProps {
  label?: LabelType | null; // null for create mode
}

export type LabelEditResult = 'saved' | 'canceled';

const LabelEditDialogImpl = NiceModal.create<LabelEditDialogProps>(
  ({ label }) => {
    const modal = useModal();
    const { t } = useTranslation('settings');
    const [formData, setFormData] = useState({
      name: '',
      icon: 'tag',
      color: '#6b7280',
    });
    const [saving, setSaving] = useState(false);
    const [error, setError] = useState<string | null>(null);

    const isEditMode = Boolean(label);

    useEffect(() => {
      if (label) {
        setFormData({
          name: label.name,
          icon: label.icon,
          color: label.color,
        });
      } else {
        setFormData({
          name: '',
          icon: 'tag',
          color: '#6b7280',
        });
      }
      setError(null);
    }, [label]);

    // Create preview label for display
    const previewLabel: LabelType = {
      id: label?.id || 'preview',
      project_id: null,
      name: formData.name || 'Label Name',
      icon: formData.icon,
      color: formData.color,
      created_at: new Date().toISOString(),
      updated_at: new Date().toISOString(),
    };

    const handleSave = async () => {
      if (!formData.name.trim()) {
        setError(t('settings.general.labels.dialog.errors.nameRequired'));
        return;
      }

      setSaving(true);
      setError(null);

      try {
        if (isEditMode && label) {
          const updateData: UpdateLabel = {
            name: formData.name,
            icon: formData.icon,
            color: formData.color,
          };
          await labelsApi.update(label.id, updateData);
        } else {
          const createData: CreateLabel = {
            project_id: null, // Global label
            name: formData.name,
            icon: formData.icon,
            color: formData.color,
          };
          await labelsApi.create(createData);
        }

        modal.resolve('saved' as LabelEditResult);
        modal.hide();
      } catch (err: unknown) {
        setError(
          getErrorMessage(err) ||
            t('settings.general.labels.dialog.errors.saveFailed')
        );
      } finally {
        setSaving(false);
      }
    };

    const handleCancel = () => {
      modal.resolve('canceled' as LabelEditResult);
      modal.hide();
    };

    const handleOpenChange = (open: boolean) => {
      if (!open) {
        setFormData({
          name: '',
          icon: 'tag',
          color: '#6b7280',
        });
        setError(null);
        handleCancel();
      }
    };

    return (
      <Dialog open={modal.visible} onOpenChange={handleOpenChange}>
        <DialogContent className="sm:max-w-[450px]">
          <DialogHeader>
            <DialogTitle>
              {isEditMode
                ? t('settings.general.labels.dialog.editTitle')
                : t('settings.general.labels.dialog.createTitle')}
            </DialogTitle>
          </DialogHeader>
          <div className="space-y-4 py-4">
            {/* Live Preview */}
            <div className="flex items-center justify-center p-4 bg-muted/30 rounded-lg">
              <LabelBadge label={previewLabel} size="md" />
            </div>

            <div>
              <Label htmlFor="label-name">
                {t('settings.general.labels.dialog.name.label')}{' '}
                <span className="text-destructive">*</span>
              </Label>
              <Input
                id="label-name"
                value={formData.name}
                onChange={(e) =>
                  setFormData({ ...formData, name: e.target.value })
                }
                placeholder={t(
                  'settings.general.labels.dialog.name.placeholder'
                )}
                disabled={saving}
                autoFocus
                className="mt-1.5"
              />
            </div>

            <div>
              <Label htmlFor="label-icon">
                {t('settings.general.labels.dialog.icon.label')}
              </Label>
              <div className="mt-1.5">
                <IconPicker
                  value={formData.icon}
                  onChange={(icon) => setFormData({ ...formData, icon })}
                  disabled={saving}
                />
              </div>
            </div>

            <div>
              <Label htmlFor="label-color">
                {t('settings.general.labels.dialog.color.label')}
              </Label>
              <div className="mt-1.5">
                <ColorPicker
                  value={formData.color}
                  onChange={(color) => setFormData({ ...formData, color })}
                  disabled={saving}
                />
              </div>
            </div>

            {error && <Alert variant="destructive">{error}</Alert>}
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={handleCancel} disabled={saving}>
              {t('settings.general.labels.dialog.buttons.cancel')}
            </Button>
            <Button
              onClick={handleSave}
              disabled={saving || !formData.name.trim()}
            >
              {saving && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
              {isEditMode
                ? t('settings.general.labels.dialog.buttons.update')
                : t('settings.general.labels.dialog.buttons.create')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    );
  }
);

export const LabelEditDialog = defineModal<
  LabelEditDialogProps,
  LabelEditResult
>(LabelEditDialogImpl);
