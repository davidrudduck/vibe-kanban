import React, { createContext, useContext, useEffect, useState } from 'react';
import { hexToHslChannels, deriveAccentVariants } from '@/lib/colorUtils';

type AccentProviderProps = {
  children: React.ReactNode;
  initialAccent?: string | null;
};

type AccentProviderState = {
  accentColor: string | null;
  setAccentColor: (color: string | null) => void;
};

const AccentProviderContext = createContext<AccentProviderState>({
  accentColor: null,
  setAccentColor: () => null,
});

export function AccentProvider({
  children,
  initialAccent,
}: AccentProviderProps) {
  const [accentColor, setAccentColorState] = useState<string | null>(
    initialAccent ?? null
  );

  // Update when initialAccent changes (config loaded), skip if value is identical
  useEffect(() => {
    const next = initialAccent ?? null;
    if (next !== accentColor) {
      setAccentColorState(next);
    }
  }, [initialAccent]); // eslint-disable-line react-hooks/exhaustive-deps

  // Apply accent when it changes
  useEffect(() => {
    applyAccent(accentColor);
  }, [accentColor]);

  const setAccentColor = (color: string | null) => {
    setAccentColorState(color);
  };

  return (
    <AccentProviderContext.Provider value={{ accentColor, setAccentColor }}>
      {children}
    </AccentProviderContext.Provider>
  );
}

export const useAccent = () => {
  const context = useContext(AccentProviderContext);
  if (context === undefined) {
    throw new Error('useAccent must be used within an AccentProvider');
  }
  return context;
};

/**
 * Apply an accent colour to CSS variables on :root.
 * Accepts either an HSL channels string ("H S% L%") or a hex string ("#rrggbb").
 * Pass null to reset to the CSS-defined default.
 */
export function applyAccent(colorOrNull: string | null): void {
  const root = document.documentElement;

  if (!colorOrNull) {
    // Remove overrides — CSS file defaults take over
    root.style.removeProperty('--brand');
    root.style.removeProperty('--brand-hover');
    root.style.removeProperty('--brand-secondary');
    return;
  }

  // Determine if it's a hex or already HSL channels
  let hslChannels: string | null;
  if (colorOrNull.startsWith('#')) {
    hslChannels = hexToHslChannels(colorOrNull);
  } else {
    // Assume it's already "H S% L%" format
    hslChannels = colorOrNull;
  }

  if (!hslChannels) return;

  const { hover, secondary } = deriveAccentVariants(hslChannels);
  root.style.setProperty('--brand', hslChannels);
  root.style.setProperty('--brand-hover', hover);
  root.style.setProperty('--brand-secondary', secondary);
}
