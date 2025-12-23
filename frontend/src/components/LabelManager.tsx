import { useState, useEffect, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import { Plus, Edit2, Trash2, Loader2 } from 'lucide-react';
import { labelsApi } from '@/lib/api';
import { LabelEditDialog } from '@/components/dialogs/settings/LabelEditDialog';
import { LabelBadge } from '@/components/labels';
import type { Label } from 'shared/types';

export function LabelManager() {
  const { t } = useTranslation('settings');
  const [labels, setLabels] = useState<Label[]>([]);
  const [loading, setLoading] = useState(true);

  const fetchLabels = useCallback(async () => {
    setLoading(true);
    try {
      // Fetch only global labels for settings
      const data = await labelsApi.list();
      setLabels(data);
    } catch (err) {
      console.error('Failed to fetch labels:', err);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchLabels();
  }, [fetchLabels]);

  const handleOpenDialog = useCallback(
    async (label?: Label) => {
      try {
        const result = await LabelEditDialog.show({
          label: label || null,
        });

        if (result === 'saved') {
          await fetchLabels();
        }
      } catch {
        // User cancelled - do nothing
      }
    },
    [fetchLabels]
  );

  const handleDelete = useCallback(
    async (label: Label) => {
      if (
        !confirm(
          t('settings.general.labels.manager.deleteConfirm', {
            labelName: label.name,
          })
        )
      ) {
        return;
      }

      try {
        await labelsApi.delete(label.id);
        await fetchLabels();
      } catch (err) {
        console.error('Failed to delete label:', err);
      }
    },
    [fetchLabels, t]
  );

  if (loading) {
    return (
      <div className="flex items-center justify-center py-8">
        <Loader2 className="h-8 w-8 animate-spin" />
      </div>
    );
  }

  return (
    <div className="space-y-4">
      <div className="flex justify-between items-center">
        <h3 className="text-lg font-semibold">
          {t('settings.general.labels.manager.title')}
        </h3>
        <Button onClick={() => handleOpenDialog()} size="sm">
          <Plus className="h-4 w-4 mr-2" />
          {t('settings.general.labels.manager.addLabel')}
        </Button>
      </div>

      {labels.length === 0 ? (
        <div className="text-center py-8 text-muted-foreground">
          {t('settings.general.labels.manager.noLabels')}
        </div>
      ) : (
        <div className="border rounded-lg overflow-hidden">
          <div className="max-h-[400px] overflow-auto">
            <table className="w-full">
              <thead className="border-b bg-muted/50 sticky top-0">
                <tr>
                  <th className="text-left p-2 text-sm font-medium">
                    {t('settings.general.labels.manager.table.preview')}
                  </th>
                  <th className="text-left p-2 text-sm font-medium">
                    {t('settings.general.labels.manager.table.name')}
                  </th>
                  <th className="text-left p-2 text-sm font-medium">
                    {t('settings.general.labels.manager.table.icon')}
                  </th>
                  <th className="text-left p-2 text-sm font-medium">
                    {t('settings.general.labels.manager.table.color')}
                  </th>
                  <th className="text-right p-2 text-sm font-medium">
                    {t('settings.general.labels.manager.table.actions')}
                  </th>
                </tr>
              </thead>
              <tbody>
                {labels.map((label) => (
                  <tr
                    key={label.id}
                    className="border-b hover:bg-muted/30 transition-colors"
                  >
                    <td className="p-2">
                      <LabelBadge label={label} size="sm" />
                    </td>
                    <td className="p-2 text-sm">{label.name}</td>
                    <td className="p-2 text-sm font-mono text-muted-foreground">
                      {label.icon}
                    </td>
                    <td className="p-2">
                      <div className="flex items-center gap-2">
                        <div
                          className="h-4 w-4 rounded border border-border"
                          style={{ backgroundColor: label.color }}
                        />
                        <span className="text-sm font-mono text-muted-foreground">
                          {label.color}
                        </span>
                      </div>
                    </td>
                    <td className="p-2">
                      <div className="flex justify-end gap-1">
                        <Button
                          variant="ghost"
                          size="icon"
                          className="h-7 w-7"
                          onClick={() => handleOpenDialog(label)}
                          title={t(
                            'settings.general.labels.manager.actions.editLabel'
                          )}
                        >
                          <Edit2 className="h-3 w-3" />
                        </Button>
                        <Button
                          variant="ghost"
                          size="icon"
                          className="h-7 w-7"
                          onClick={() => handleDelete(label)}
                          title={t(
                            'settings.general.labels.manager.actions.deleteLabel'
                          )}
                        >
                          <Trash2 className="h-3 w-3" />
                        </Button>
                      </div>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      )}
    </div>
  );
}
