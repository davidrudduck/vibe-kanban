import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { cloneDeep, isEqual } from 'lodash';
import { CheckIcon } from '@phosphor-icons/react';
import {
  AppearanceConfig,
  CodeFont,
  ProseFont,
  ThemeMode,
  UiFont,
} from 'shared/types';
import { useUserSystem } from '@/shared/hooks/useUserSystem';
import { useTheme } from '@/shared/hooks/useTheme';
import { useFonts } from '@/shared/components/FontProvider';
import { applyAccent } from '@/shared/components/AccentProvider';
import { hexToHslChannels } from '@/lib/colorUtils';
import {
  getCodeFontFamily,
  getProseFontFamily,
  getUiFontFamily,
} from '@/lib/fonts';
import { cn } from '@/shared/lib/utils';
import { PrimaryButton } from '@vibe/ui/components/PrimaryButton';
import {
  SettingsCard,
  SettingsCheckbox,
  SettingsField,
  SettingsInput,
  SettingsSaveBar,
  SettingsSelect,
} from './SettingsComponents';
import { useSettingsDirty } from './SettingsDirtyContext';

const ACCENT_PRESETS: { label: string; value: string }[] = [
  { label: 'Orange (default)', value: '25 82% 54%' },
  { label: 'Blue', value: '217 91% 60%' },
  { label: 'Purple', value: '271 81% 60%' },
  { label: 'Green', value: '142 71% 45%' },
  { label: 'Red', value: '0 72% 51%' },
];

export function AppearanceSettings() {
  const { t } = useTranslation(['settings', 'common']);
  const { config, loading, updateAndSaveConfig } = useUserSystem();
  const { setDirty: setContextDirty } = useSettingsDirty();
  const { setTheme } = useTheme();
  const { setFonts } = useFonts();

  const [draft, setDraft] = useState(() => (config ? cloneDeep(config) : null));
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [success, setSuccess] = useState(false);
  const [hexInput, setHexInput] = useState('');
  const [hexError, setHexError] = useState(false);

  // Sync draft when config loads
  useEffect(() => {
    if (config && !draft) {
      setDraft(cloneDeep(config));
    }
  }, [config, draft]);

  const isDirty =
    draft && config ? !isEqual(draft.appearance, config.appearance) : false;

  // Track dirty state in context
  useEffect(() => {
    setContextDirty('appearance', isDirty);
    return () => setContextDirty('appearance', false);
  }, [isDirty, setContextDirty]);

  const updateAppearance = (updates: Partial<AppearanceConfig>) => {
    setDraft((prev) =>
      prev
        ? {
            ...prev,
            appearance: { ...prev.appearance, ...updates },
          }
        : prev
    );
  };

  const handleSave = async () => {
    if (!draft) return;
    setSaving(true);
    setError(null);
    setSuccess(false);
    try {
      await updateAndSaveConfig(draft);
      // Apply live updates after save
      setFonts(draft.appearance.fonts);
      applyAccent(draft.appearance.accent_color);
      setSuccess(true);
      setTimeout(() => setSuccess(false), 2000);
      setContextDirty('appearance', false);
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Failed to save');
    } finally {
      setSaving(false);
    }
  };

  const handleCancel = () => {
    if (config) setDraft(cloneDeep(config));
    setHexInput('');
    setHexError(false);
    setContextDirty('appearance', false);
    // Revert live preview
    if (config) {
      applyAccent(config.appearance.accent_color);
      setFonts(config.appearance.fonts);
    }
  };

  if (loading || !draft) {
    return (
      <div className="py-8 text-low text-sm">
        {t('settings.appearance.loading', { defaultValue: 'Loading…' })}
      </div>
    );
  }

  const themeOptions: ThemeMode[] = [
    ThemeMode.LIGHT,
    ThemeMode.DARK,
    ThemeMode.SYSTEM,
  ];

  const handleAccentSelect = (value: string | null) => {
    updateAppearance({ accent_color: value });
    applyAccent(value);
    setHexInput('');
    setHexError(false);
  };

  const handleHexChange = (value: string) => {
    setHexInput(value);
    if (!value) {
      setHexError(false);
      return;
    }
    const hsl = hexToHslChannels(value);
    if (hsl) {
      setHexError(false);
      updateAppearance({ accent_color: hsl });
      applyAccent(hsl);
    } else {
      setHexError(true);
    }
  };

  const handleFontChange = (fonts: typeof draft.appearance.fonts) => {
    updateAppearance({ fonts });
    setFonts(fonts);
  };

  return (
    <div className="flex flex-col h-full">
      {error && (
        <div className="bg-error/10 border border-error/50 rounded-sm p-4 text-error mb-4">
          {error}
        </div>
      )}
      {success && (
        <div className="bg-success/10 border border-success/50 rounded-sm p-4 text-success font-medium mb-4">
          {t('settings.appearance.saveSuccess', {
            defaultValue: 'Appearance saved',
          })}
        </div>
      )}

      <div className="flex gap-4 flex-1 min-h-0">
        {/* Settings form */}
        <div className="flex-1 min-w-0 space-y-6">
          {/* Theme & Colour */}
          <SettingsCard
            title={t('settings.appearance.theme.title', {
              defaultValue: 'Theme & Colour',
            })}
            description={t('settings.appearance.theme.description', {
              defaultValue:
                'Choose your colour mode and the accent used across the UI.',
            })}
          >
            <SettingsField
              label={t('settings.appearance.theme.modeLabel', {
                defaultValue: 'Theme mode',
              })}
            >
              <div className="flex gap-2">
                {themeOptions.map((mode) => {
                  const selected = draft.theme === mode;
                  return (
                    <button
                      key={mode}
                      type="button"
                      onClick={() => {
                        setTheme(mode);
                        setDraft((prev) =>
                          prev ? { ...prev, theme: mode } : prev
                        );
                      }}
                      className={cn(
                        'flex-1 px-base py-half rounded-sm border text-sm capitalize transition-colors',
                        selected
                          ? 'border-brand bg-brand/10 text-brand font-medium'
                          : 'border-border bg-secondary text-normal hover:bg-secondary/80'
                      )}
                    >
                      {mode.toLowerCase()}
                    </button>
                  );
                })}
              </div>
            </SettingsField>

            <SettingsField
              label={t('settings.appearance.theme.accentLabel', {
                defaultValue: 'Accent colour',
              })}
              description={t('settings.appearance.theme.accentHelper', {
                defaultValue:
                  'Pick a preset or enter a custom hex value (e.g. #e07b2a).',
              })}
            >
              <div className="space-y-3">
                <div className="flex items-center gap-2 flex-wrap">
                  {ACCENT_PRESETS.map((preset) => {
                    const selected =
                      draft.appearance.accent_color === preset.value;
                    return (
                      <button
                        key={preset.value}
                        type="button"
                        title={preset.label}
                        onClick={() => handleAccentSelect(preset.value)}
                        className={cn(
                          'w-6 h-6 rounded-full border-2 flex items-center justify-center transition-all',
                          selected
                            ? 'border-high scale-110'
                            : 'border-border hover:scale-105'
                        )}
                        style={{ background: `hsl(${preset.value})` }}
                      >
                        {selected && (
                          <CheckIcon
                            className="size-icon-sm text-on-brand"
                            weight="bold"
                          />
                        )}
                      </button>
                    );
                  })}
                  <PrimaryButton
                    variant="tertiary"
                    value={t('settings.appearance.theme.accentClear', {
                      defaultValue: 'Reset',
                    })}
                    onClick={() => handleAccentSelect(null)}
                  />
                </div>
                <SettingsInput
                  value={hexInput}
                  onChange={handleHexChange}
                  placeholder="#e07b2a"
                  error={hexError}
                />
              </div>
            </SettingsField>
          </SettingsCard>

          {/* Fonts */}
          <SettingsCard
            title={t('settings.appearance.fonts.title', {
              defaultValue: 'Fonts',
            })}
            description={t('settings.appearance.fonts.description', {
              defaultValue:
                'Select fonts for UI, code blocks, and prose content.',
            })}
          >
            <SettingsField
              label={t('settings.appearance.fonts.uiLabel', {
                defaultValue: 'UI font',
              })}
            >
              <SettingsSelect<UiFont>
                value={draft.appearance.fonts.ui_font}
                options={[
                  { value: 'IBM_PLEX_SANS', label: 'IBM Plex Sans' },
                  { value: 'INTER', label: 'Inter' },
                  { value: 'ROBOTO', label: 'Roboto' },
                  { value: 'PUBLIC_SANS', label: 'Public Sans' },
                  { value: 'SYSTEM', label: 'System' },
                ]}
                onChange={(value) =>
                  handleFontChange({
                    ...draft.appearance.fonts,
                    ui_font: value,
                  })
                }
              />
            </SettingsField>

            <SettingsField
              label={t('settings.appearance.fonts.codeLabel', {
                defaultValue: 'Code font',
              })}
            >
              <SettingsSelect<CodeFont>
                value={draft.appearance.fonts.code_font}
                options={[
                  { value: 'IBM_PLEX_MONO', label: 'IBM Plex Mono' },
                  { value: 'JET_BRAINS_MONO', label: 'JetBrains Mono' },
                  { value: 'CASCADIA_MONO', label: 'Cascadia Mono' },
                  { value: 'HACK', label: 'Hack' },
                  { value: 'SYSTEM', label: 'System' },
                ]}
                onChange={(value) =>
                  handleFontChange({
                    ...draft.appearance.fonts,
                    code_font: value,
                  })
                }
              />
            </SettingsField>

            <SettingsField
              label={t('settings.appearance.fonts.proseLabel', {
                defaultValue: 'Prose font',
              })}
            >
              <SettingsSelect<ProseFont>
                value={draft.appearance.fonts.prose_font}
                options={[
                  { value: 'IBM_PLEX_SANS', label: 'IBM Plex Sans' },
                  { value: 'ROBOTO', label: 'Roboto' },
                  { value: 'GEORGIA', label: 'Georgia' },
                  { value: 'SYSTEM', label: 'System' },
                ]}
                onChange={(value) =>
                  handleFontChange({
                    ...draft.appearance.fonts,
                    prose_font: value,
                  })
                }
              />
            </SettingsField>

            <SettingsCheckbox
              id="disable-ligatures"
              label={t('settings.appearance.fonts.disableLigatures', {
                defaultValue: 'Disable ligatures',
              })}
              checked={draft.appearance.fonts.disable_ligatures}
              onChange={(checked) =>
                handleFontChange({
                  ...draft.appearance.fonts,
                  disable_ligatures: checked,
                })
              }
            />

            <div className="mt-2 space-y-1 text-sm text-normal border border-border rounded-sm p-3 bg-secondary/30">
              <div
                style={{
                  fontFamily: getUiFontFamily(draft.appearance.fonts.ui_font),
                }}
              >
                UI: The quick brown fox (Aa Bb 0123)
              </div>
              <div
                style={{
                  fontFamily: getCodeFontFamily(
                    draft.appearance.fonts.code_font
                  ),
                  fontVariantLigatures: draft.appearance.fonts.disable_ligatures
                    ? 'none'
                    : 'normal',
                }}
              >
                Code: const answer = 42; // ligatures: -&gt; =&gt;
              </div>
              <div
                style={{
                  fontFamily: getProseFontFamily(
                    draft.appearance.fonts.prose_font
                  ),
                }}
              >
                Prose: Task description text, readable at length.
              </div>
            </div>
          </SettingsCard>

          {/* Host Banner */}
          <SettingsCard
            title={t('settings.appearance.hostBanner.title', {
              defaultValue: 'Host Banner',
            })}
            description={t('settings.appearance.hostBanner.description', {
              defaultValue:
                'Configure the banner shown in the top of the app shell.',
            })}
          >
            <SettingsCheckbox
              id="show-hostname"
              label={t('settings.appearance.hostBanner.showHostname', {
                defaultValue: 'Show hostname',
              })}
              checked={draft.appearance.host_banner.show_hostname}
              onChange={(checked) =>
                updateAppearance({
                  host_banner: {
                    ...draft.appearance.host_banner,
                    show_hostname: checked,
                  },
                })
              }
            />
            <SettingsCheckbox
              id="show-os-info"
              label={t('settings.appearance.hostBanner.showOsInfo', {
                defaultValue: 'Show OS info',
              })}
              checked={draft.appearance.host_banner.show_os_info}
              onChange={(checked) =>
                updateAppearance({
                  host_banner: {
                    ...draft.appearance.host_banner,
                    show_os_info: checked,
                  },
                })
              }
            />
            <SettingsField
              label={t('settings.appearance.hostBanner.envLabel', {
                defaultValue: 'Environment label',
              })}
            >
              <SettingsInput
                value={draft.appearance.host_banner.env_label ?? ''}
                onChange={(value) =>
                  updateAppearance({
                    host_banner: {
                      ...draft.appearance.host_banner,
                      env_label: value || null,
                    },
                  })
                }
                placeholder="dev / prod / staging"
              />
            </SettingsField>
          </SettingsCard>

          {/* Links & Community */}
          <SettingsCard
            title={t('settings.appearance.links.title', {
              defaultValue: 'Links & Community',
            })}
            description={t('settings.appearance.links.description', {
              defaultValue: 'Customise external links shown in the app.',
            })}
          >
            <SettingsCheckbox
              id="discord-enabled"
              label={t('settings.appearance.links.discordEnabled', {
                defaultValue: 'Show Discord link',
              })}
              checked={draft.appearance.links.discord_enabled}
              onChange={(checked) =>
                updateAppearance({
                  links: {
                    ...draft.appearance.links,
                    discord_enabled: checked,
                  },
                })
              }
            />
            {draft.appearance.links.discord_enabled && (
              <SettingsField
                label={t('settings.appearance.links.discordUrl', {
                  defaultValue: 'Discord URL',
                })}
              >
                <SettingsInput
                  value={draft.appearance.links.discord_url ?? ''}
                  onChange={(value) =>
                    updateAppearance({
                      links: {
                        ...draft.appearance.links,
                        discord_url: value || null,
                      },
                    })
                  }
                  placeholder="https://discord.gg/..."
                />
              </SettingsField>
            )}
            <SettingsField
              label={t('settings.appearance.links.feedbackUrl', {
                defaultValue: 'Feedback URL',
              })}
            >
              <SettingsInput
                value={draft.appearance.links.feedback_url ?? ''}
                onChange={(value) =>
                  updateAppearance({
                    links: {
                      ...draft.appearance.links,
                      feedback_url: value || null,
                    },
                  })
                }
                placeholder="https://github.com/owner/repo/issues"
              />
            </SettingsField>
          </SettingsCard>
        </div>

        {/* Live preview pane */}
        <div className="w-[340px] flex-shrink-0 hidden xl:block">
          <div className="sticky top-0 space-y-3 p-3 border border-border rounded-sm bg-secondary/30">
            <div className="text-sm font-medium text-low mb-2">
              {t('settings.appearance.preview.title', {
                defaultValue: 'Preview',
              })}
            </div>

            {(draft.appearance.host_banner.show_hostname ||
              draft.appearance.host_banner.env_label) && (
              <div className="flex items-center gap-2 px-2 py-1 rounded-sm bg-panel text-xs text-normal">
                <span className="text-brand">●</span>
                {draft.appearance.host_banner.show_hostname && (
                  <span>hostname</span>
                )}
                {draft.appearance.host_banner.env_label && (
                  <span className="px-1 rounded bg-brand/20 text-brand text-xs">
                    {draft.appearance.host_banner.env_label}
                  </span>
                )}
              </div>
            )}

            <div className="flex items-center gap-2">
              {draft.appearance.links.discord_enabled && (
                <div className="px-2 py-1 rounded-sm bg-panel text-xs text-normal">
                  Discord
                </div>
              )}
              <div className="px-2 py-1 rounded-sm bg-panel text-xs text-normal">
                Feedback
              </div>
            </div>

            <div className="space-y-1 border border-border rounded-sm p-2 bg-primary">
              <div
                className="text-sm font-medium text-high"
                style={{
                  fontFamily: getUiFontFamily(draft.appearance.fonts.ui_font),
                }}
              >
                UI Font — Navigation & Labels
              </div>
              <div
                className="text-xs text-normal"
                style={{
                  fontFamily: getProseFontFamily(
                    draft.appearance.fonts.prose_font
                  ),
                }}
              >
                Prose font for task descriptions and markdown.
              </div>
              <div
                className="text-xs text-low"
                style={{
                  fontFamily: getCodeFontFamily(
                    draft.appearance.fonts.code_font
                  ),
                  fontVariantLigatures: draft.appearance.fonts.disable_ligatures
                    ? 'none'
                    : 'normal',
                }}
              >
                {'const x = () => { return 42; }'}
              </div>
            </div>

            <div className="flex items-center gap-2">
              <div className="w-6 h-6 rounded-full bg-brand flex-shrink-0" />
              <span className="text-xs text-low">Brand accent</span>
              <div className="ml-auto">
                <button className="px-2 py-1 rounded-sm bg-brand text-on-brand text-xs">
                  Button
                </button>
              </div>
            </div>
          </div>
        </div>
      </div>

      <SettingsSaveBar
        show={isDirty}
        saving={saving}
        onSave={handleSave}
        onDiscard={handleCancel}
      />
    </div>
  );
}
