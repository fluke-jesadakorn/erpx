'use client';

import { useState } from 'react';
import { T } from '@/components/i18n/T';

export function FinanceRagPanel() {
  const [q, setQ] = useState('');
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<any>(null);
  const [error, setError] = useState<string | null>(null);

  const onAsk = async () => {
    if (!q.trim() || busy) return;
    setBusy(true);
    setError(null);
    setResult(null);
    try {
      const res = await fetch('/api/finance/rag', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ question: q, lang: 'en' }),
      });
      const json = await res.json();
      if (!res.ok || !json.ok) {
        setError(json.error ?? `HTTP ${res.status}`);
      } else {
        setResult(json);
      }
    } catch (e: any) {
      setError(e?.message ?? String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <section className="rounded-2xl border border-emerald-500/30 bg-emerald-950/15 p-5">
      <h3 className="mb-2 text-xs font-mono uppercase tracking-widest text-emerald-300">
        <T id="cockpit.ragVendor" />
      </h3>
      <p className="mb-3 text-xs text-slate-500">
        <T id="cockpit.ragVendorHint" />
      </p>
      <textarea
        value={q}
        onChange={(e) => setQ(e.target.value)}
        rows={2}
        placeholder="e.g. which vendor charged the most for travel this month?"
        className="w-full rounded-lg border border-slate-700 bg-slate-950 px-3 py-2 text-sm text-white placeholder-slate-500 focus:border-emerald-500 focus:outline-none"
      />
      <button
        type="button"
        onClick={onAsk}
        disabled={busy || !q.trim()}
        className="mt-2 rounded-lg border border-emerald-500/40 bg-emerald-500/10 px-4 py-1.5 text-xs font-mono text-emerald-200 hover:bg-emerald-500/20 disabled:opacity-50"
      >
        {busy ? <T id="cockpit.asking" /> : <T id="cockpit.ask" />}
      </button>
      {error && <div className="mt-3 rounded border border-rose-500/40 bg-rose-500/10 px-3 py-2 text-xs text-rose-200">{error}</div>}
      {result && (
        <div className="mt-3 space-y-2">
          {result.answer && <p className="whitespace-pre-wrap text-sm leading-relaxed text-slate-200">{result.answer}</p>}
          {Array.isArray(result.hits) && result.hits.length > 0 && (
            <div className="flex flex-wrap gap-1.5 pt-1">
              {result.hits.slice(0, 6).map((h: any, i: number) => (
                <a
                  key={i}
                  href={`/waybill/WB-${h.expense_id}`}
                  className="rounded-full border border-slate-700 bg-slate-900 px-2 py-0.5 text-xs font-mono text-slate-300 hover:border-emerald-500 hover:text-emerald-200"
                >
                  EXP-{h.expense_id} · {h.vendor_name ?? '?'} · {(Number(h.score) * 100).toFixed(0)}%
                </a>
              ))}
            </div>
          )}
        </div>
      )}
    </section>
  );
}