'use client';

import { useFormStatus } from 'react-dom';
import { T } from '@/components/i18n/T';

interface Props {
  label: string;
  pendingLabel: string;
  pendingHint: string;
  className: string;
  icon?: string;
  testId?: string;
}

export function GlSubmit({
  label,
  pendingLabel,
  pendingHint,
  className,
  icon,
  testId,
}: Props) {
  const { pending } = useFormStatus();

  return (
    <div className="max-w-full">
      <button
        type="submit"
        disabled={pending}
        aria-busy={pending}
        data-testid={testId}
        className={`inline-flex min-h-10 max-w-full items-center justify-center gap-2 transition disabled:cursor-wait disabled:opacity-80 ${className}`}
      >
        {pending ? (
          <span className="size-4 shrink-0 animate-spin rounded-full border-2 border-current border-t-transparent motion-reduce:animate-none" aria-hidden />
        ) : icon ? (
          <span className="shrink-0" aria-hidden>{icon}</span>
        ) : null}
        <span className="min-w-0">
          <T
            id={pending ? pendingLabel : label}
            variant="compact"
            primaryClassName="font-semibold"
            secondaryClassName="ml-1.5 text-xs font-normal opacity-80"
          />
        </span>
      </button>
      <div className={pending ? 'mt-2 flex max-w-xl items-start gap-2 rounded-md border border-info/40 bg-info-soft px-3 py-2 text-xs text-ink-2' : 'sr-only'} role="status" aria-live="polite">
        {pending && (
          <>
            <span className="mt-0.5 size-3 shrink-0 animate-pulse rounded-full bg-info motion-reduce:animate-none" aria-hidden />
            <T
              id={pendingHint}
              variant="stacked"
              primaryClassName="block font-medium leading-relaxed text-ink-2"
              secondaryClassName="mt-0.5 block font-normal leading-relaxed text-mute"
            />
          </>
        )}
      </div>
    </div>
  );
}
