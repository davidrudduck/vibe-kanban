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
import type { SwarmProject } from '@/types/swarm';

interface SwarmProjectDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  project?: SwarmProject | null;
  onSave: (data: { name: string; description: string | null }) => Promise<void>;
  isSaving: boolean;
}

export function SwarmProjectDialog({
  open,
  onOpenChange,
  project,
  onSave,
  isSaving,
}: SwarmProjectDialogProps) {
  const { t } = useTranslation(['settings', 'common']);
  const [name, setName] = useState('');
  const [description, setDescription] = useState('');
  const [error, setError] = useState<string | null>(null);

  const isEditing = !!project;

  // Reset form when dialog opens/closes or project changes
  useEffect(() => {
    if (open) {
      if (project) {
        setName(project.name);
        setDescription(project.description || '');
      } else {
        setName('');
        setDescription('');
      }
      setError(null);
    }
  }, [open, project]);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError(null);

    const trimmedName = name.trim();
    if (!trimmedName) {
      setError(
        t(
          'settings.swarm.projects.dialog.nameRequired',
          'Project name is required'
        )
      );
      return;
    }

    try {
      await onSave({
        name: trimmedName,
        description: description.trim() || null,
      });
      onOpenChange(false);
    } catch (err) {
      const message =
        err instanceof Error ? err.message : 'An error occurred';
      setError(message);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md">
        <form onSubmit={handleSubmit}>
          <DialogHeader>
            <DialogTitle>
              {isEditing
                ? t(
                    'settings.swarm.projects.dialog.editTitle',
                    'Edit Swarm Project'
                  )
                : t(
                    'settings.swarm.projects.dialog.createTitle',
                    'Create Swarm Project'
                  )}
            </DialogTitle>
            <DialogDescription>
              {isEditing
                ? t(
                    'settings.swarm.projects.dialog.editDescription',
                    'Update the project name and description.'
                  )
                : t(
                    'settings.swarm.projects.dialog.createDescription',
                    'Create a new swarm project that can be linked to node projects.'
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
              <Label htmlFor="project-name">
                {t('settings.swarm.projects.dialog.nameLabel', 'Name')}
              </Label>
              <Input
                id="project-name"
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder={t(
                  'settings.swarm.projects.dialog.namePlaceholder',
                  'e.g., My Shared Project'
                )}
                disabled={isSaving}
                autoFocus
              />
            </div>

            <div className="space-y-2">
              <Label htmlFor="project-description">
                {t(
                  'settings.swarm.projects.dialog.descriptionLabel',
                  'Description'
                )}
                <span className="text-muted-foreground font-normal ml-1">
                  ({t('common:optional', 'optional')})
                </span>
              </Label>
              <Textarea
                id="project-description"
                value={description}
                onChange={(e) => setDescription(e.target.value)}
                placeholder={t(
                  'settings.swarm.projects.dialog.descriptionPlaceholder',
                  'Brief description of the project'
                )}
                disabled={isSaving}
                rows={3}
              />
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
            <Button type="submit" disabled={isSaving || !name.trim()}>
              {isSaving && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
              {isEditing
                ? t('settings.swarm.projects.dialog.save', 'Save Changes')
                : t('settings.swarm.projects.dialog.create', 'Create Project')}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
