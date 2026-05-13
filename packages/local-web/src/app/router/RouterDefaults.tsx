import { useRouter } from '@tanstack/react-router';
import { WarningIcon, ArrowClockwiseIcon } from '@phosphor-icons/react';

export function RouterPendingComponent() {
  return (
    <div className="flex h-screen w-screen items-center justify-center bg-primary">
      <div className="size-6 animate-spin rounded-full border-2 border-muted border-t-brand" />
    </div>
  );
}

export function RouterErrorComponent({ error }: { error: Error }) {
  const router = useRouter();
  return (
    <div className="flex h-screen w-screen items-center justify-center bg-primary p-double">
      <div className="flex max-w-sm flex-col items-center gap-double text-center">
        <WarningIcon className="size-12 text-error" weight="fill" />
        <div className="flex flex-col gap-half">
          <h1 className="text-xl font-semibold text-high">
            Something went wrong
          </h1>
          <p className="text-sm text-low">
            {error?.message ?? 'An unexpected error occurred.'}
          </p>
        </div>
        <button
          type="button"
          onClick={() => router.invalidate()}
          className="flex items-center gap-half rounded-md bg-brand px-double py-base text-sm font-medium text-white hover:bg-brand/90 transition-colors"
        >
          <ArrowClockwiseIcon className="size-icon-base" weight="bold" />
          Try again
        </button>
      </div>
    </div>
  );
}
