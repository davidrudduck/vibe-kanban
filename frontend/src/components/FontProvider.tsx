import React, { createContext, useContext, useEffect, useState } from 'react';
import { FontConfig } from 'shared/types';
import {
  loadFont,
  getUiFontFamily,
  getCodeFontFamily,
  getProseFontFamily,
  getUiFontUrl,
  getCodeFontUrl,
  getProseFontUrl,
} from '@/lib/fonts';

type FontProviderProps = {
  children: React.ReactNode;
  initialFonts?: FontConfig;
};

type FontProviderState = {
  fonts: FontConfig;
  setFonts: (fonts: FontConfig) => void;
};

const defaultFonts: FontConfig = {
  ui_font: 'INTER',
  code_font: 'JET_BRAINS_MONO',
  prose_font: 'INTER',
  disable_ligatures: false,
};

const FontProviderContext = createContext<FontProviderState>({
  fonts: defaultFonts,
  setFonts: () => null,
});

export function FontProvider({ children, initialFonts }: FontProviderProps) {
  const [fonts, setFontsState] = useState<FontConfig>(
    initialFonts || defaultFonts
  );

  // Update when initialFonts changes (config loaded)
  useEffect(() => {
    if (initialFonts) {
      setFontsState(initialFonts);
    }
  }, [initialFonts]);

  // Apply fonts when they change
  useEffect(() => {
    const root = document.documentElement;

    // Load font files if needed
    loadFont(getUiFontUrl(fonts.ui_font));
    loadFont(getCodeFontUrl(fonts.code_font));
    loadFont(getProseFontUrl(fonts.prose_font));

    // Set CSS variables
    root.style.setProperty('--font-ui', getUiFontFamily(fonts.ui_font));
    root.style.setProperty('--font-code', getCodeFontFamily(fonts.code_font));
    root.style.setProperty(
      '--font-prose',
      getProseFontFamily(fonts.prose_font)
    );
    root.style.setProperty(
      '--font-ligatures',
      fonts.disable_ligatures ? 'none' : 'normal'
    );
  }, [fonts]);

  const setFonts = (newFonts: FontConfig) => {
    setFontsState(newFonts);
  };

  return (
    <FontProviderContext.Provider value={{ fonts, setFonts }}>
      {children}
    </FontProviderContext.Provider>
  );
}

export const useFonts = () => {
  const context = useContext(FontProviderContext);
  if (context === undefined) {
    throw new Error('useFonts must be used within a FontProvider');
  }
  return context;
};
