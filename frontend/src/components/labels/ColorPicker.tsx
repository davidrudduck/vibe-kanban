import { useState } from 'react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from '@/components/ui/popover';
import { cn } from '@/lib/utils';

// Predefined color palette - carefully selected for good contrast and accessibility
const PRESET_COLORS = [
  // Row 1 - Neutral
  '#6b7280', // gray
  '#374151', // dark gray
  '#1f2937', // charcoal
  // Row 2 - Warm
  '#ef4444', // red
  '#f97316', // orange
  '#f59e0b', // amber
  '#eab308', // yellow
  // Row 3 - Cool
  '#22c55e', // green
  '#14b8a6', // teal
  '#06b6d4', // cyan
  '#3b82f6', // blue
  // Row 4 - Purple/Pink
  '#6366f1', // indigo
  '#8b5cf6', // violet
  '#a855f7', // purple
  '#ec4899', // pink
];

interface ColorPickerProps {
  value: string;
  onChange: (color: string) => void;
  disabled?: boolean;
}

export function ColorPicker({ value, onChange, disabled }: ColorPickerProps) {
  const [open, setOpen] = useState(false);
  const [customColor, setCustomColor] = useState(value);

  const handleCustomColorChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const newColor = e.target.value;
    setCustomColor(newColor);
    // Only apply if it's a valid hex color
    if (/^#[0-9A-Fa-f]{6}$/.test(newColor)) {
      onChange(newColor);
    }
  };

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <Button
          variant="outline"
          className="w-full justify-start gap-2"
          disabled={disabled}
        >
          <div
            className="h-5 w-5 rounded border border-border"
            style={{ backgroundColor: value }}
          />
          <span className="font-mono text-sm">{value}</span>
        </Button>
      </PopoverTrigger>
      <PopoverContent className="w-64 p-3" align="start">
        <div className="space-y-3">
          {/* Preset colors grid */}
          <div className="grid grid-cols-8 gap-1.5">
            {PRESET_COLORS.map((color) => (
              <button
                key={color}
                type="button"
                className={cn(
                  'h-6 w-6 rounded border-2 transition-all hover:scale-110',
                  value === color
                    ? 'border-foreground ring-2 ring-foreground ring-offset-2'
                    : 'border-transparent'
                )}
                style={{ backgroundColor: color }}
                onClick={() => {
                  onChange(color);
                  setCustomColor(color);
                }}
                title={color}
              />
            ))}
          </div>

          {/* Custom color input */}
          <div className="flex items-center gap-2">
            <Input
              type="text"
              value={customColor}
              onChange={handleCustomColorChange}
              placeholder="#000000"
              className="font-mono text-sm h-8"
            />
            <input
              type="color"
              value={value}
              onChange={(e) => {
                onChange(e.target.value);
                setCustomColor(e.target.value);
              }}
              className="h-8 w-8 cursor-pointer rounded border border-border"
            />
          </div>
        </div>
      </PopoverContent>
    </Popover>
  );
}
