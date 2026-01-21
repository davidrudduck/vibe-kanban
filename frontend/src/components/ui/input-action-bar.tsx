import { Button } from '@/components/ui/button';
import { Image as ImageIcon, FileText, X, Send } from 'lucide-react';
import { cn } from '@/lib/utils';
import { VariantSelector } from '@/components/tasks/VariantSelector';
import type { ExecutorConfig } from 'shared/types';
import type { ReactNode } from 'react';

interface InputActionBarProps {
  // Left side - media/template actions
  showImageButton?: boolean;
  imageActive?: boolean;
  onImageClick?: () => void;
  imageDisabled?: boolean;

  showTemplateButton?: boolean;
  onTemplateClick?: () => void;
  templateDisabled?: boolean;

  // Right side - variant selector (optional)
  showVariantSelector?: boolean;
  variant?: string | null;
  onVariantChange?: (variant: string | null) => void;
  variantProfile?: ExecutorConfig | null;
  variantDisabled?: boolean;

  // Right side - cancel (optional)
  showCancel?: boolean;
  onCancel?: () => void;
  cancelDisabled?: boolean;
  cancelLabel?: string;

  // Right side - primary action (required)
  primaryLabel: string;
  onPrimary: () => void;
  primaryDisabled?: boolean;
  primaryIcon?: ReactNode;
  primaryVariant?: 'default' | 'destructive';

  className?: string;
}

export function InputActionBar({
  showImageButton,
  imageActive,
  onImageClick,
  imageDisabled,
  showTemplateButton,
  onTemplateClick,
  templateDisabled,
  showVariantSelector,
  variant,
  onVariantChange,
  variantProfile,
  variantDisabled,
  showCancel,
  onCancel,
  cancelDisabled,
  cancelLabel = 'Cancel',
  primaryLabel,
  onPrimary,
  primaryDisabled,
  primaryIcon,
  primaryVariant = 'default',
  className,
}: InputActionBarProps) {
  return (
    <div className={cn('flex items-center gap-2', className)}>
      {/* Left group - Image & Template */}
      <div className="flex gap-2">
        {showImageButton && (
          <Button
            variant={imageActive ? 'default' : 'secondary'}
            size="sm"
            onClick={onImageClick}
            disabled={imageDisabled}
          >
            <ImageIcon className="h-4 w-4" />
          </Button>
        )}
        {showTemplateButton && (
          <Button
            variant="secondary"
            size="sm"
            onClick={onTemplateClick}
            disabled={templateDisabled}
          >
            <FileText className="h-4 w-4" />
          </Button>
        )}
      </div>

      {/* Spacer */}
      <div className="flex-1" />

      {/* Right group - Variant, Cancel, Primary */}
      <div className="flex items-center gap-2">
        {showVariantSelector && onVariantChange && (
          <VariantSelector
            selectedVariant={variant ?? null}
            onChange={onVariantChange}
            currentProfile={variantProfile ?? null}
            disabled={variantDisabled}
          />
        )}
        {showCancel && (
          <Button
            variant="outline"
            size="sm"
            onClick={onCancel}
            disabled={cancelDisabled}
          >
            <X className="h-3 w-3 mr-1" />
            {cancelLabel}
          </Button>
        )}
        <Button
          variant={primaryVariant}
          size="sm"
          onClick={onPrimary}
          disabled={primaryDisabled}
        >
          {primaryIcon || <Send className="h-3 w-3 mr-1" />}
          {primaryLabel}
        </Button>
      </div>
    </div>
  );
}
