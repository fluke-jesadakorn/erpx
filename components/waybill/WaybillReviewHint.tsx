'use client';

import { useState } from 'react';
import type { ReactNode } from 'react';
import { T } from '@/components/i18n/T';

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
  label: ReactNode;
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
    <section className="rounded-md border border-info bg-info-soft p-4">
      <div className="mb-2 flex items-center gap-2">
        <h3 className="text-xs font-mono uppercase tracking-widest text-info-strong">{label}</h3>
        <button
          type="button"
          onClick={onAsk}
          disabled={busy}
          className="ml-auto rounded-lg border border-info bg-info px-3 py-1 text-xs font-mono text-paper hover:bg-info disabled:opacity-50"
        >
          {busy ? <T id="waybill.review.thinking" /> : hint ? <T id="waybill.review.refresh" /> : <T id="waybill.review.generate" />}
        </button>
      </div>
      {error && <div className="rounded border border-critical bg-critical-soft px-3 py-2 text-xs text-critical-strong">{error}</div>}
      {!error && !hint && (
        <p className="text-xs text-mute">
          <T id="waybill.review.placeholder" />
        </p>
      )}
      {hint && (
        <p className="whitespace-pre-wrap text-sm leading-relaxed text-ink">{hint.hint}</p>
      )}
    </section>
  );
}
