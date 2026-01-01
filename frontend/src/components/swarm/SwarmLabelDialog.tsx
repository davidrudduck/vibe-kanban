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
import { Alert, AlertDescription } from '@/components/ui/alert';
import { ColorPicker } from '@/components/labels/ColorPicker';
import { IconPicker } from '@/components/labels/IconPicker';
import type { SwarmLabel } from '@/types/swarm';

interface SwarmLabelDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  label?: SwarmLabel | null;
  onSave: (data: {
    name: string;
    color: string;
    icon: string | null;
  }) => Promise<void>;
  isSaving: boolean;
}

// Default color for new labels
const DEFAULT_COLOR = '#3b82f6';
const DEFAULT_ICON = 'tag';

export function SwarmLabelDialog({
  open,
  onOpenChange,
  label,
  onSave,
  isSaving,
}: SwarmLabelDialogProps) {
  const { t } = useTranslation(['settings', 'common']);
  const [name, setName] = useState('');
  const [color, setColor] = useState(DEFAULT_COLOR);
  const [icon, setIcon] = useState(DEFAULT_ICON);
  const [error, setError] = useState<string | null>(null);

  const isEditing = !!label;

  // Reset form when dialog opens/closes or label changes
  useEffect(() => {
    if (open) {
      if (label) {
        setName(label.name);
        setColor(label.color);
        setIcon(label.icon || DEFAULT_ICON);
      } else {
        setName('');
        setColor(DEFAULT_COLOR);
        setIcon(DEFAULT_ICON);
      }
      setError(null);
    }
  }, [open, label]);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError(null);

    const trimmedName = name.trim();
    if (!trimmedName) {
      setError(
        t('settings.swarm.labels.dialog.nameRequired', 'Label name is required')
      );
      return;
    }

    try {
      await onSave({
        name: trimmedName,
        color,
        icon: icon || null,
      });
      onOpenChange(false);
    } catch (err) {
      const message = err instanceof Error ? err.message : 'An error occurred';
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
                    'settings.swarm.labels.dialog.editTitle',
                    'Edit Swarm Label'
                  )
                : t(
                    'settings.swarm.labels.dialog.createTitle',
                    'Create Swarm Label'
                  )}
            </DialogTitle>
            <DialogDescription>
              {isEditing
                ? t(
                    'settings.swarm.labels.dialog.editDescription',
                    'Update the label name, color, and icon.'
                  )
                : t(
                    'settings.swarm.labels.dialog.createDescription',
                    'Create a new organization-wide label for categorizing tasks.'
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
              <Label htmlFor="label-name">
                {t('settings.swarm.labels.dialog.nameLabel', 'Name')}
              </Label>
              <Input
                id="label-name"
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder={t(
                  'settings.swarm.labels.dialog.namePlaceholder',
                  'e.g., Bug, Feature, Documentation'
                )}
                disabled={isSaving}
                autoFocus
              />
            </div>

            <div className="space-y-2">
              <Label htmlFor="label-color">
                {t('settings.swarm.labels.dialog.colorLabel', 'Color')}
              </Label>
              <ColorPicker
                value={color}
                onChange={setColor}
                disabled={isSaving}
              />
            </div>

            <div className="space-y-2">
              <Label htmlFor="label-icon">
                {t('settings.swarm.labels.dialog.iconLabel', 'Icon')}
                <span className="text-muted-foreground font-normal ml-1">
                  ({t('common:optional', 'optional')})
                </span>
              </Label>
              <IconPicker value={icon} onChange={setIcon} disabled={isSaving} />
            </div>

            {/* Preview */}
            <div className="space-y-2">
              <Label>
                {t('settings.swarm.labels.dialog.preview', 'Preview')}
              </Label>
              <div className="flex items-center gap-2 p-3 bg-muted/50 rounded-md">
                <span
                  className="inline-flex items-center gap-1.5 px-2 py-1 rounded-full text-sm font-medium"
                  style={{
                    backgroundColor: color,
                    color: getContrastColor(color),
                  }}
                >
                  {icon && <IconDisplay iconName={icon} />}
                  {name ||
                    t(
                      'settings.swarm.labels.dialog.previewPlaceholder',
                      'Label Name'
                    )}
                </span>
              </div>
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
                ? t('settings.swarm.labels.dialog.save', 'Save Changes')
                : t('settings.swarm.labels.dialog.create', 'Create Label')}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

// Helper to calculate contrasting text color
function getContrastColor(hexColor: string): string {
  const hex = hexColor.replace('#', '');
  const r = parseInt(hex.substring(0, 2), 16);
  const g = parseInt(hex.substring(2, 4), 16);
  const b = parseInt(hex.substring(4, 6), 16);
  const luminance = (0.299 * r + 0.587 * g + 0.114 * b) / 255;
  return luminance > 0.5 ? '#000000' : '#ffffff';
}

// Simple icon display component for preview
import { getLucideIcon } from '@/components/labels/IconPicker';

function IconDisplay({ iconName }: { iconName: string }) {
  const IconComponent = getLucideIcon(iconName);
  if (!IconComponent) return null;
  return <IconComponent className="h-3.5 w-3.5" />;
}
