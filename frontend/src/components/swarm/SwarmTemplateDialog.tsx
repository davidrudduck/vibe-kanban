import { useState, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { Loader2 } from 'lucide-react';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Textarea } from '@/components/ui/textarea';
import { Alert, AlertDescription } from '@/components/ui/alert';
import type { SwarmTemplate } from '@/types/swarm';

interface SwarmTemplateDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  template?: SwarmTemplate | null;
  onSave: (data: { name: string; content: string }) => Promise<void>;
  isSaving: boolean;
}

export function SwarmTemplateDialog({
  open,
  onOpenChange,
  template,
  onSave,
  isSaving,
}: SwarmTemplateDialogProps) {
  const { t } = useTranslation(['settings', 'common']);
  const [name, setName] = useState('');
  const [content, setContent] = useState('');
  const [error, setError] = useState<string | null>(null);

  const isEditing = !!template;

  // Reset form when dialog opens/closes or template changes
  useEffect(() => {
    if (open) {
      if (template) {
        setName(template.name);
        setContent(template.content);
      } else {
        setName('');
        setContent('');
      }
      setError(null);
    }
  }, [open, template]);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError(null);

    const trimmedName = name.trim();
    const trimmedContent = content.trim();

    if (!trimmedName) {
      setError(
        t(
          'settings.swarm.templates.dialog.nameRequired',
          'Template name is required'
        )
      );
      return;
    }

    if (!trimmedContent) {
      setError(
        t(
          'settings.swarm.templates.dialog.contentRequired',
          'Template content is required'
        )
      );
      return;
    }

    try {
      await onSave({
        name: trimmedName,
        content: trimmedContent,
      });
      onOpenChange(false);
    } catch (err) {
      const message = err instanceof Error ? err.message : 'An error occurred';
      setError(message);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-lg">
        <form onSubmit={handleSubmit}>
          <DialogHeader>
            <DialogTitle>
              {isEditing
                ? t(
                    'settings.swarm.templates.dialog.editTitle',
                    'Edit Swarm Template'
                  )
                : t(
                    'settings.swarm.templates.dialog.createTitle',
                    'Create Swarm Template'
                  )}
            </DialogTitle>
            <DialogDescription>
              {isEditing
                ? t(
                    'settings.swarm.templates.dialog.editDescription',
                    'Update the template name and content.'
                  )
                : t(
                    'settings.swarm.templates.dialog.createDescription',
                    'Create a reusable template that can be inserted into task descriptions using @mentions.'
                  )}
            </DialogDescription>
          </DialogHeader>

          <div className="space-y-4 py-4">
            {error && (
              <Alert variant="destructive">
                <AlertDescription>{error}</AlertDescription>
              </Alert>
            )}

            <div className="space-y-2">
              <Label htmlFor="template-name">
                {t('settings.swarm.templates.dialog.nameLabel', 'Name')}
              </Label>
              <div className="flex items-center gap-2">
                <span className="text-muted-foreground">@</span>
                <Input
                  id="template-name"
                  value={name}
                  onChange={(e) => setName(e.target.value)}
                  placeholder={t(
                    'settings.swarm.templates.dialog.namePlaceholder',
                    'e.g., testing-guidelines'
                  )}
                  disabled={isSaving}
                  autoFocus
                  className="flex-1"
                />
              </div>
              <p className="text-xs text-muted-foreground">
                {t(
                  'settings.swarm.templates.dialog.nameHelper',
                  'Use @{{name}} in task descriptions to insert this template.',
                  { name: name || 'name' }
                )}
              </p>
            </div>

            <div className="space-y-2">
              <Label htmlFor="template-content">
                {t('settings.swarm.templates.dialog.contentLabel', 'Content')}
              </Label>
              <Textarea
                id="template-content"
                value={content}
                onChange={(e) => setContent(e.target.value)}
                placeholder={t(
                  'settings.swarm.templates.dialog.contentPlaceholder',
                  'Enter the template content...'
                )}
                disabled={isSaving}
                rows={8}
                className="font-mono text-sm"
              />
              <p className="text-xs text-muted-foreground">
                {t(
                  'settings.swarm.templates.dialog.contentHelper',
                  'This content will be expanded when the template is referenced.'
                )}
              </p>
            </div>
          </div>

          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              onClick={() => onOpenChange(false)}
              disabled={isSaving}
            >
              {t('common:cancel', 'Cancel')}
            </Button>
            <Button
              type="submit"
              disabled={isSaving || !name.trim() || !content.trim()}
            >
              {isSaving && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
              {isEditing
                ? t('settings.swarm.templates.dialog.save', 'Save Changes')
                : t(
                    'settings.swarm.templates.dialog.create',
                    'Create Template'
                  )}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
