/**
 * Convert a 3 or 6-digit hex colour string to HSL channel string.
 * Returns "H S% L%" format (no hsl() wrapper) for use with CSS vars.
 * Returns null if hex is invalid.
 */
export function hexToHslChannels(hex: string): string | null {
  // Normalise: strip # prefix, expand 3-digit shorthand
  const clean = hex.replace(/^#/, '');
  let r: number, g: number, b: number;

  if (clean.length === 3) {
    r = parseInt(clean[0] + clean[0], 16);
    g = parseInt(clean[1] + clean[1], 16);
    b = parseInt(clean[2] + clean[2], 16);
  } else if (clean.length === 6) {
    r = parseInt(clean.slice(0, 2), 16);
    g = parseInt(clean.slice(2, 4), 16);
    b = parseInt(clean.slice(4, 6), 16);
  } else {
    return null;
  }

  if (isNaN(r) || isNaN(g) || isNaN(b)) return null;

  r /= 255;
  g /= 255;
  b /= 255;

  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  const delta = max - min;
  let h = 0;
  let s = 0;
  const l = (max + min) / 2;

  if (delta !== 0) {
    s = delta / (1 - Math.abs(2 * l - 1));
    switch (max) {
      case r:
        h = ((g - b) / delta + (g < b ? 6 : 0)) / 6;
        break;
      case g:
        h = ((b - r) / delta + 2) / 6;
        break;
      case b:
        h = ((r - g) / delta + 4) / 6;
        break;
    }
  }

  const hDeg = Math.round(h * 360);
  const sPct = Math.round(s * 100);
  const lPct = Math.round(l * 100);

  return `${hDeg} ${sPct}% ${lPct}%`;
}

/**
 * Given an HSL channel string ("H S% L%"), derive hover (+8% lightness)
 * and secondary (–17% lightness) variants.
 */
export function deriveAccentVariants(hslChannels: string): {
  hover: string;
  secondary: string;
} {
  const match = hslChannels.match(/^(\d+)\s+(\d+)%\s+(\d+)%$/);
  if (!match) {
    return { hover: hslChannels, secondary: hslChannels };
  }

  const h = parseInt(match[1]);
  const s = parseInt(match[2]);
  const l = parseInt(match[3]);

  const hover = `${h} ${Math.min(s + 0, 100)}% ${Math.min(l + 8, 100)}%`;
  const secondary = `${h} ${s}% ${Math.max(l - 17, 0)}%`;

  return { hover, secondary };
}
