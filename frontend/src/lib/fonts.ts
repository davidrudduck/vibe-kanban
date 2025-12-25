import { UiFont, CodeFont, ProseFont } from 'shared/types';

// Google Fonts URLs for each font
const UI_FONT_URLS: Record<UiFont, string | null> = {
  INTER:
    'https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600;700&display=swap',
  ROBOTO:
    'https://fonts.googleapis.com/css2?family=Roboto:wght@400;500;700&display=swap',
  PUBLIC_SANS:
    'https://fonts.googleapis.com/css2?family=Public+Sans:wght@400;500;600;700&display=swap',
  CHIVO_MONO: null, // Already loaded in defaults
  SYSTEM: null,
};

const CODE_FONT_URLS: Record<CodeFont, string | null> = {
  JET_BRAINS_MONO:
    'https://fonts.googleapis.com/css2?family=JetBrains+Mono:wght@400;500;600;700&display=swap',
  CASCADIA_MONO: null, // Not on Google Fonts - use system fallback
  HACK: null, // Not on Google Fonts - use system fallback
  IBM_PLEX_MONO:
    'https://fonts.googleapis.com/css2?family=IBM+Plex+Mono:wght@400;500;600;700&display=swap',
  CHIVO_MONO: null, // Already loaded
  SYSTEM: null,
};

const PROSE_FONT_URLS: Record<ProseFont, string | null> = {
  INTER: null, // Same as UI - already loaded
  ROBOTO: null, // Same as UI - already loaded if used
  GEORGIA: null, // System font
  CHIVO_MONO: null,
  SYSTEM: null,
};

// Font family CSS values
const UI_FONT_FAMILIES: Record<UiFont, string> = {
  INTER: "'Inter', 'Noto Emoji', sans-serif",
  ROBOTO: "'Roboto', 'Noto Emoji', sans-serif",
  PUBLIC_SANS: "'Public Sans', 'Noto Emoji', sans-serif",
  CHIVO_MONO: "'Chivo Mono', 'Noto Emoji', monospace",
  SYSTEM:
    "-apple-system, BlinkMacSystemFont, 'Segoe UI', 'Noto Emoji', sans-serif",
};

const CODE_FONT_FAMILIES: Record<CodeFont, string> = {
  JET_BRAINS_MONO: "'JetBrains Mono', monospace",
  CASCADIA_MONO: "'Cascadia Mono', 'Cascadia Code', monospace",
  HACK: "'Hack', monospace",
  IBM_PLEX_MONO: "'IBM Plex Mono', monospace",
  CHIVO_MONO: "'Chivo Mono', monospace",
  SYSTEM: "ui-monospace, SFMono-Regular, 'SF Mono', Consolas, monospace",
};

const PROSE_FONT_FAMILIES: Record<ProseFont, string> = {
  INTER: "'Inter', 'Noto Emoji', sans-serif",
  ROBOTO: "'Roboto', 'Noto Emoji', sans-serif",
  GEORGIA: "Georgia, 'Times New Roman', serif",
  CHIVO_MONO: "'Chivo Mono', 'Noto Emoji', monospace",
  SYSTEM:
    "-apple-system, BlinkMacSystemFont, 'Segoe UI', 'Noto Emoji', sans-serif",
};

// Track loaded fonts to avoid duplicate loading
const loadedFonts = new Set<string>();

export function loadFont(url: string | null): void {
  if (!url || loadedFonts.has(url)) return;

  const link = document.createElement('link');
  link.rel = 'stylesheet';
  link.href = url;
  document.head.appendChild(link);
  loadedFonts.add(url);
}

export function getUiFontFamily(font: UiFont): string {
  return UI_FONT_FAMILIES[font];
}

export function getCodeFontFamily(font: CodeFont): string {
  return CODE_FONT_FAMILIES[font];
}

export function getProseFontFamily(font: ProseFont): string {
  return PROSE_FONT_FAMILIES[font];
}

export function getUiFontUrl(font: UiFont): string | null {
  return UI_FONT_URLS[font];
}

export function getCodeFontUrl(font: CodeFont): string | null {
  return CODE_FONT_URLS[font];
}

export function getProseFontUrl(font: ProseFont): string | null {
  return PROSE_FONT_URLS[font];
}
