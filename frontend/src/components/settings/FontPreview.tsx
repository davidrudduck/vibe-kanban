import { useTranslation } from 'react-i18next';
import { FontConfig } from 'shared/types';
import {
  getUiFontFamily,
  getCodeFontFamily,
  getProseFontFamily,
} from '@/lib/fonts';

type FontPreviewProps = {
  fonts: FontConfig;
};

function FontPreview({ fonts }: FontPreviewProps) {
  const { t } = useTranslation('settings');

  const uiFontFamily = getUiFontFamily(fonts.ui_font);
  const codeFontFamily = getCodeFontFamily(fonts.code_font);
  const proseFontFamily = getProseFontFamily(fonts.prose_font);
  const ligatureStyle = fonts.disable_ligatures ? 'none' : 'normal';

  return (
    <div className="border rounded-lg p-4 space-y-4 bg-muted/30">
      <p className="text-sm font-medium text-muted-foreground">
        {t('settings.general.fonts.preview.title')}
      </p>

      <div className="space-y-3">
        {/* UI Font Preview */}
        <div className="space-y-1">
          <span className="text-xs text-muted-foreground">
            {t('settings.general.fonts.uiFont.label')}
          </span>
          <p style={{ fontFamily: uiFontFamily }} className="text-base">
            {t('settings.general.fonts.preview.uiSample')}
          </p>
        </div>

        {/* Code Font Preview */}
        <div className="space-y-1">
          <span className="text-xs text-muted-foreground">
            {t('settings.general.fonts.codeFont.label')}
          </span>
          <pre
            style={{
              fontFamily: codeFontFamily,
              fontVariantLigatures: ligatureStyle,
            }}
            className="text-sm bg-muted/50 p-2 rounded overflow-x-auto"
          >
            {t('settings.general.fonts.preview.codeSample')}
          </pre>
        </div>

        {/* Prose Font Preview */}
        <div className="space-y-1">
          <span className="text-xs text-muted-foreground">
            {t('settings.general.fonts.proseFont.label')}
          </span>
          <p style={{ fontFamily: proseFontFamily }} className="text-base">
            {t('settings.general.fonts.preview.proseSample')}
          </p>
        </div>
      </div>
    </div>
  );
}

export default FontPreview;
