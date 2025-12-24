import { useState, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Textarea } from '@/components/ui/textarea';
import { Alert } from '@/components/ui/alert';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from '@/components/ui/dialog';
import { Loader2 } from 'lucide-react';
import { templatesApi } from '@/lib/api';
import type { Template, CreateTemplate, UpdateTemplate } from 'shared/types';
import NiceModal, { useModal } from '@ebay/nice-modal-react';
import { defineModal, getErrorMessage } from '@/lib/modals';

export interface TemplateEditDialogProps {
  template?: Template | null; // null for create mode
}

export type TemplateEditResult = 'saved' | 'canceled';

const TemplateEditDialogImpl = NiceModal.create<TemplateEditDialogProps>(
  ({ template }) => {
    const modal = useModal();
    const { t } = useTranslation('settings');
    const [formData, setFormData] = useState({
      template_name: '',
      content: '',
    });
    const [saving, setSaving] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const [templateNameError, setTemplateNameError] = useState<string | null>(
      null
    );

    const isEditMode = Boolean(template);

    useEffect(() => {
      if (template) {
        setFormData({
          template_name: template.template_name,
          content: template.content,
        });
      } else {
        setFormData({
          template_name: '',
          content: '',
        });
      }
      setError(null);
      setTemplateNameError(null);
    }, [template]);

    const handleSave = async () => {
      if (!formData.template_name.trim()) {
        setError(t('settings.general.templates.dialog.errors.nameRequired'));
        return;
      }

      setSaving(true);
      setError(null);

      try {
        if (isEditMode && template) {
          const updateData: UpdateTemplate = {
            template_name: formData.template_name,
            content: formData.content || null, // null means "don't update"
          };
          await templatesApi.update(template.id, updateData);
        } else {
          const createData: CreateTemplate = {
            template_name: formData.template_name,
            content: formData.content,
          };
          await templatesApi.create(createData);
        }

        modal.resolve('saved' as TemplateEditResult);
        modal.hide();
      } catch (err: unknown) {
        setError(
          getErrorMessage(err) ||
            t('settings.general.templates.dialog.errors.saveFailed')
        );
      } finally {
        setSaving(false);
      }
    };

    const handleCancel = () => {
      modal.resolve('canceled' as TemplateEditResult);
      modal.hide();
    };

    const handleOpenChange = (open: boolean) => {
      if (!open) {
        // Reset form data when dialog closes
        setFormData({
          template_name: '',
          content: '',
        });
        setError(null);
        handleCancel();
      }
    };

    return (
      <Dialog open={modal.visible} onOpenChange={handleOpenChange}>
        <DialogContent className="sm:max-w-[500px]">
          <DialogHeader>
            <DialogTitle>
              {isEditMode
                ? t('settings.general.templates.dialog.editTitle')
                : t('settings.general.templates.dialog.createTitle')}
            </DialogTitle>
          </DialogHeader>
          <div className="space-y-4 py-4">
            <div>
              <Label htmlFor="template-name">
                {t('settings.general.templates.dialog.templateName.label')}{' '}
                <span className="text-destructive">
                  {t('settings.general.templates.dialog.templateName.required')}
                </span>
              </Label>
              <p className="text-xs text-muted-foreground mb-1.5">
                {t('settings.general.templates.dialog.templateName.hint', {
                  templateName: formData.template_name || 'template_name',
                })}
              </p>
              <Input
                id="template-name"
                value={formData.template_name}
                onChange={(e) => {
                  const value = e.target.value;
                  setFormData({ ...formData, template_name: value });

                  // Validate in real-time for spaces
                  if (value.includes(' ')) {
                    setTemplateNameError(
                      t('settings.general.templates.dialog.templateName.error')
                    );
                  } else {
                    setTemplateNameError(null);
                  }
                }}
                placeholder={t(
                  'settings.general.templates.dialog.templateName.placeholder'
                )}
                disabled={saving}
                autoFocus
                aria-invalid={!!templateNameError}
                className={templateNameError ? 'border-destructive' : undefined}
              />
              {templateNameError && (
                <p className="text-sm text-destructive">{templateNameError}</p>
              )}
            </div>
            <div>
              <Label htmlFor="template-content">
                {t('settings.general.templates.dialog.content.label')}{' '}
                <span className="text-destructive">
                  {t('settings.general.templates.dialog.content.required')}
                </span>
              </Label>
              <p className="text-xs text-muted-foreground mb-1.5">
                {t('settings.general.templates.dialog.content.hint', {
                  templateName: formData.template_name || 'template_name',
                })}
              </p>
              <Textarea
                id="template-content"
                value={formData.content}
                onChange={(e) => {
                  const value = e.target.value;
                  setFormData({ ...formData, content: value });
                }}
                placeholder={t(
                  'settings.general.templates.dialog.content.placeholder'
                )}
                rows={6}
                disabled={saving}
              />
            </div>
            {error && <Alert variant="destructive">{error}</Alert>}
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={handleCancel} disabled={saving}>
              {t('settings.general.templates.dialog.buttons.cancel')}
            </Button>
            <Button
              onClick={handleSave}
              disabled={
                saving || !!templateNameError || !formData.content.trim()
              }
            >
              {saving && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
              {isEditMode
                ? t('settings.general.templates.dialog.buttons.update')
                : t('settings.general.templates.dialog.buttons.create')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    );
  }
);

export const TemplateEditDialog = defineModal<
  TemplateEditDialogProps,
  TemplateEditResult
>(TemplateEditDialogImpl);
