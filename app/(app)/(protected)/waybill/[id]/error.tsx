'use client';

interface Props {
  error: Error & { digest?: string };
  reset: () => void;
}

export default function WaybillError({ error, reset }: Props) {
  return (
    <div className="rounded-md border border-critical/40 bg-critical-soft p-6 text-sm text-critical">
      <div className="text-base font-bold text-critical">Could not load waybill</div>
      <p className="mt-2 font-mono text-xs text-critical/90">
        {error.message || 'Unknown error'}
        {error.digest ? ` · digest ${error.digest}` : ''}
      </p>
      <button
        type="button"
        onClick={reset}
        className="mt-4 rounded-lg border border-critical/40 bg-critical px-4 py-2 text-critical hover:bg-critical"
      >
        Retry
      </button>
    </div>
  );
}
