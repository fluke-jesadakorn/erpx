'use client';

import { useState } from 'react';

interface Hint {
  waybillId: string;
  stage: 'hod' | 'am';
  hint: string;
  generatedAt: string;
}

interface Props {
  waybillId: string;
  lang?: 'en' | 'th' | 'de';
  stage: 'hod' | 'am';
  label: string;
}

export function WaybillReviewHint({ waybillId, lang = 'en', stage, label }: Props) {
  const [busy, setBusy] = useState(false);
  const [hint, setHint] = useState<Hint | null>(null);
  const [error, setError] = useState<string | null>(null);

  const onAsk = async () => {
    if (busy) return;
    setBusy(true);
    setError(null);
    setHint(null);
    try {
      const res = await fetch(`/api/waybill/${encodeURIComponent(waybillId)}/review-hint`, {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ stage, lang }),
      });
      const json = await res.json();
      if (!res.ok || !json.ok) {
        setError(json.error ?? `HTTP ${res.status}`);
        setHint(null);
      } else {
        setHint(json.hint);
      }
    } catch (e: any) {
      setError(e?.message ?? String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <section className="rounded-2xl border border-teal-500/30 bg-teal-950/15 p-4">
      <div className="mb-2 flex items-center gap-2">
        <h3 className="text-xs font-mono uppercase tracking-widest text-teal-300">{label}</h3>
        <button
          type="button"
          onClick={onAsk}
          disabled={busy}
          className="ml-auto rounded-lg border border-teal-500/40 bg-teal-500/10 px-3 py-1 text-xs font-mono text-teal-200 hover:bg-teal-500/20 disabled:opacity-50"
        >
          {busy ? 'Thinking…' : hint ? 'Refresh' : 'Generate'}
        </button>
      </div>
      {error && <div className="rounded border border-rose-500/40 bg-rose-500/10 px-3 py-2 text-xs text-rose-200">{error}</div>}
      {!error && !hint && (
        <p className="text-xs text-slate-500">Click Generate to get an AI review hint for this stage.</p>
      )}
      {hint && (
        <p className="whitespace-pre-wrap text-sm leading-relaxed text-slate-200">{hint.hint}</p>
      )}
    </section>
  );
}
