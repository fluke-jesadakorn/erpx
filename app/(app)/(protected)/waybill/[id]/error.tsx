'use client';

interface Props {
  error: Error & { digest?: string };
  reset: () => void;
}

export default function WaybillError({ error, reset }: Props) {
  return (
    <div className="rounded-2xl border border-rose-500/40 bg-rose-950/30 p-6 text-sm text-rose-100">
      <div className="text-base font-bold text-rose-200">Could not load waybill</div>
      <p className="mt-2 font-mono text-xs text-rose-300/90">
        {error.message || 'Unknown error'}
        {error.digest ? ` · digest ${error.digest}` : ''}
      </p>
      <button
        type="button"
        onClick={reset}
        className="mt-4 rounded-lg border border-rose-500/40 bg-rose-500/15 px-4 py-2 text-rose-100 hover:bg-rose-500/30"
      >
        Retry
      </button>
    </div>
  );
}
