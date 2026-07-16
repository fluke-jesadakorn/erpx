'use client';

import { useState } from 'react';

export function GlCommentaryCard() {
  const [code, setCode] = useState('');
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<any>(null);
  const [error, setError] = useState<string | null>(null);

  const onAsk = async () => {
    if (!code.trim() || busy) return;
    setBusy(true);
    setError(null);
    setResult(null);
    try {
      const res = await fetch(`/api/ledger/commentary?code=${encodeURIComponent(code.trim())}`);
      const json = await res.json();
      if (!res.ok || !json.ok) {
        setError(json.error ?? `HTTP ${res.status}`);
      } else {
        setResult(json.commentary);
      }
    } catch (e: any) {
      setError(e?.message ?? String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <section className="rounded-2xl border border-violet-500/30 bg-violet-950/20 p-5">
      <h3 className="mb-2 text-xs font-mono uppercase tracking-widest text-violet-300">GL Line Commentary</h3>
      <p className="mb-3 text-xs text-slate-500">Explain a chart-of-account balance in plain language.</p>
      <div className="flex gap-2">
        <input
          type="text"
          value={code}
          onChange={(e) => setCode(e.target.value)}
          placeholder="account code (e.g. 510200)"
          className="flex-1 rounded-lg border border-slate-700 bg-slate-950 px-3 py-2 text-sm font-mono text-white placeholder-slate-500 focus:border-violet-500 focus:outline-none"
        />
        <button
          type="button"
          onClick={onAsk}
          disabled={busy || !code.trim()}
          className="rounded-lg border border-violet-500/40 bg-violet-500/10 px-4 py-1.5 text-xs font-mono text-violet-200 hover:bg-violet-500/20 disabled:opacity-50"
        >
          {busy ? 'Asking…' : 'Explain'}
        </button>
      </div>
      {error && <div className="mt-3 rounded border border-rose-500/40 bg-rose-500/10 px-3 py-2 text-xs text-rose-200">{error}</div>}
      {result && (
        <div className="mt-3 space-y-2">
          {result.accountName && (
            <div className="text-xs font-mono text-slate-400">
              {result.accountCode} · {result.accountName}{result.accountNameTh ? ` · ${result.accountNameTh}` : ''}
            </div>
          )}
          <p className="whitespace-pre-wrap text-sm leading-relaxed text-slate-200">{result.commentary}</p>
        </div>
      )}
    </section>
  );
}