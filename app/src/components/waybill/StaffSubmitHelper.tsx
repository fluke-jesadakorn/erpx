'use client';

import { useState } from 'react';

interface CoaSuggestion {
  code: string;
  name: string | null;
  name_th: string | null;
  similarity: number;
}

interface VendorSuggestion {
  id: number;
  code: string;
  name: string;
}

interface HelperHint {
  description: string;
  descriptionTh: string | null;
  coaSuggestions: CoaSuggestion[];
  vendorSuggestion: VendorSuggestion | null;
  amountHypothesis: number | null;
}

interface Props {
  currentUserId: number;
  lang?: 'en' | 'th' | 'de';
  onUse?: (hint: HelperHint) => void;
}

export function StaffSubmitHelper({ currentUserId, lang = 'en', onUse }: Props) {
  const [text, setText] = useState('');
  const [busy, setBusy] = useState(false);
  const [hint, setHint] = useState<HelperHint | null>(null);
  const [error, setError] = useState<string | null>(null);

  const onAsk = async () => {
    if (!text.trim() || busy) return;
    setBusy(true);
    setError(null);
    setHint(null);
    try {
      const res = await fetch('/api/expense/helper', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ text, submitterId: currentUserId, lang }),
      });
      const json = await res.json();
      if (!res.ok || !json.ok) {
        setError(json.error ?? `HTTP ${res.status}`);
      } else if (json.hint) {
        setHint(json.hint);
      } else {
        setError('AI returned no hint.');
      }
    } catch (e: any) {
      setError(e?.message ?? String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <section className="rounded-2xl border border-sky-500/30 bg-sky-950/15 p-4">
      <h3 className="mb-1 text-xs font-mono uppercase tracking-widest text-sky-300">Need help writing this?</h3>
      <p className="mb-2 text-xs text-slate-500">Describe what you bought in plain Thai or English — AI suggests a description, COA, and vendor.</p>
      <textarea
        value={text}
        onChange={(e) => setText(e.target.value)}
        rows={2}
        placeholder="e.g. กาแฟตอนประชุมลูกค้า ABC ที่ร้านกาแฟ"
        className="w-full rounded-lg border border-slate-700 bg-slate-950 px-3 py-2 text-sm text-white placeholder-slate-500 focus:border-sky-500 focus:outline-none"
      />
      <div className="mt-2 flex gap-2">
        <button
          type="button"
          onClick={onAsk}
          disabled={busy || !text.trim()}
          className="rounded-lg border border-sky-500/40 bg-sky-500/10 px-3 py-1.5 text-xs font-mono text-sky-200 hover:bg-sky-500/20 disabled:opacity-50"
        >
          {busy ? 'Thinking…' : 'Suggest'}
        </button>
        {hint && onUse && (
          <button
            type="button"
            onClick={() => onUse(hint)}
            className="rounded-lg border border-emerald-500/40 bg-emerald-500/10 px-3 py-1.5 text-xs font-mono text-emerald-200 hover:bg-emerald-500/20"
          >
            Apply to form
          </button>
        )}
      </div>
      {error && <div className="mt-3 rounded border border-rose-500/40 bg-rose-500/10 px-3 py-2 text-xs text-rose-200">{error}</div>}
      {hint && (
        <div className="mt-3 space-y-2 text-xs">
          <div>
            <div className="font-mono text-slate-500">suggested description</div>
            <div className="text-slate-200">{hint.description}</div>
            {hint.descriptionTh && <div className="text-slate-400">{hint.descriptionTh}</div>}
          </div>
          {hint.amountHypothesis != null && (
            <div>
              <span className="font-mono text-slate-500">estimated amount:</span>{' '}
              <span className="font-mono text-emerald-300">
                {hint.amountHypothesis.toLocaleString('th-TH', { minimumFractionDigits: 2 })} THB
              </span>
            </div>
          )}
          {hint.vendorSuggestion && (
            <div>
              <span className="font-mono text-slate-500">vendor:</span>{' '}
              <span className="font-mono text-cyan-300">{hint.vendorSuggestion.code}</span>{' '}
              <span className="text-slate-300">{hint.vendorSuggestion.name}</span>
            </div>
          )}
          {Array.isArray(hint.coaSuggestions) && hint.coaSuggestions.length > 0 && (
            <div>
              <div className="mb-1 font-mono text-slate-500">COA candidates</div>
              <ul className="space-y-0.5">
                {hint.coaSuggestions.slice(0, 3).map((s, i) => (
                  <li key={i} className="flex items-center gap-2 rounded border border-slate-800 bg-slate-950 px-2 py-1">
                    <span className="font-mono text-cyan-300">{s.code}</span>
                    <span className="text-slate-300">{s.name ?? '?'}</span>
                    {s.name_th && <span className="text-slate-500">({s.name_th})</span>}
                    <span className="ml-auto font-mono text-emerald-300">{(s.similarity ?? 0).toFixed(0)}%</span>
                  </li>
                ))}
              </ul>
            </div>
          )}
        </div>
      )}
    </section>
  );
}

export default StaffSubmitHelper;