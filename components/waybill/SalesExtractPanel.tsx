'use client';

import { useState } from 'react';
import { T } from '@/components/i18n/T';

export interface ExtractedDraft {
  customer_name: string;
  customer_code: string | null;
  customer_id: number | null;
  payment_terms: string;
  items: Array<{ description: string; qty: number; unit_price: number }>;
  confidence: number;
}

interface Props {
  lang: 'en' | 'th' | 'de';
  onUse: (draft: ExtractedDraft) => void;
}

export function SalesExtractPanel({ lang, onUse }: Props) {
  const [text, setText] = useState('');
  const [busy, setBusy] = useState(false);
  const [draft, setDraft] = useState<ExtractedDraft | null>(null);
  const [error, setError] = useState<string | null>(null);

  const onExtract = async () => {
    if (!text.trim() || busy) return;
    setBusy(true);
    setError(null);
    setDraft(null);
    try {
      const res = await fetch('/api/sales/extract', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ text, lang }),
      });
      const json = await res.json();
      if (!res.ok || !json.ok) {
        setError(json.error ?? `HTTP ${res.status}`);
      } else if (json.draft) {
        setDraft(json.draft);
      } else {
        setError('AI could not extract a draft from this text.');
      }
    } catch (e: any) {
      setError(e?.message ?? String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <section className="mt-4 rounded-2xl border border-fuchsia-500/30 bg-fuchsia-950/15 p-4">
      <h3 className="mb-1 text-xs font-mono uppercase tracking-widest text-fuchsia-300">
        <T id="waybill.salesExtract.title" />
      </h3>
      <p className="mb-2 text-xs text-slate-500">
        <T id="waybill.salesExtract.subtitle" />
      </p>
      <textarea
        value={text}
        onChange={(e) => setText(e.target.value)}
        rows={3}
        className="w-full rounded-lg border border-slate-700 bg-slate-950 px-3 py-2 text-sm text-white placeholder-slate-500 focus:border-fuchsia-500 focus:outline-none"
      />
      <div className="mt-2 flex gap-2">
        <button
          type="button"
          onClick={onExtract}
          disabled={busy || !text.trim()}
          className="rounded-lg border border-fuchsia-500/40 bg-fuchsia-500/10 px-3 py-1.5 text-xs font-mono text-fuchsia-200 hover:bg-fuchsia-500/20 disabled:opacity-50"
        >
          {busy ? <T id="waybill.salesExtract.busy" /> : <T id="waybill.salesExtract.extract" />}
        </button>
        {draft && (
          <button
            type="button"
            onClick={() => onUse(draft)}
            className="rounded-lg border border-emerald-500/40 bg-emerald-500/10 px-3 py-1.5 text-xs font-mono text-emerald-200 hover:bg-emerald-500/20"
          >
            <T id="waybill.salesExtract.useDraft" />
          </button>
        )}
      </div>
      {error && <div className="mt-3 rounded border border-rose-500/40 bg-rose-500/10 px-3 py-2 text-xs text-rose-200">{error}</div>}
      {draft && (
        <div className="mt-3 space-y-2">
          <div className="text-xs text-slate-300">
            <span className="font-mono text-slate-500">
              <T id="waybill.salesExtract.customer" />
            </span> {draft.customer_name}
            {draft.customer_code && <span className="ml-2 font-mono text-cyan-300">({draft.customer_code})</span>}
            <span className="ml-2 font-mono text-slate-500">
              <T id="waybill.salesExtract.terms" />
            </span> {draft.payment_terms}
            <span className="ml-2 font-mono text-slate-500">
              <T id="waybill.salesExtract.conf" />
            </span> {Math.round((draft.confidence ?? 0) * 100)}%
          </div>
          {Array.isArray(draft.items) && draft.items.length > 0 && (
            <ul className="space-y-1 text-xs">
              {draft.items.map((it, i) => (
                <li key={i} className="rounded border border-slate-800 bg-slate-950 px-2 py-1 font-mono text-slate-300">
                  {it.qty}× {it.description} @ {it.unit_price} = {(it.qty * it.unit_price).toLocaleString('th-TH', { minimumFractionDigits: 2 })}
                </li>
              ))}
            </ul>
          )}
        </div>
      )}
    </section>
  );
}
