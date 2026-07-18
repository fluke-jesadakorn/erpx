'use client';

import { useState } from 'react';
import { T } from '@/components/i18n/T';

export function CockpitSqlPanel() {
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
      const res = await fetch('/api/cockpit/sql', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ question: q, lang: 'en' }),
      });
      const json = await res.json();
      if (!res.ok || !json.ok) {
        setError(json.error ?? `HTTP ${res.status}`);
      } else {
        setResult(json.result);
      }
    } catch (e: any) {
      setError(e?.message ?? String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <section className="rounded-md border border-info bg-info-soft p-5">
      <h3 className="mb-2 text-xs font-mono uppercase tracking-widest text-info">
        <T id="cockpit.sqlAskTheBooks" />
      </h3>
      <p className="mb-3 text-xs text-mute">
        <T id="cockpit.sqlReadOnlyHint" />
      </p>
      <textarea
        value={q}
        onChange={(e) => setQ(e.target.value)}
        rows={2}
        placeholder="e.g. top 5 vendors by total spend this month"
        className="w-full rounded-lg border border-rule bg-paper px-3 py-2 text-sm text-ink placeholder-mute focus:border-info focus:outline-none"
      />
      <button
        type="button"
        onClick={onAsk}
        disabled={busy || !q.trim()}
        className="mt-2 rounded-lg border border-info bg-info px-4 py-1.5 text-xs font-mono text-info-soft hover:bg-info disabled:opacity-50"
      >
        {busy ? <T id="cockpit.asking" /> : <T id="cockpit.ask" />}
      </button>
      {error && <div className="mt-3 rounded border border-critical bg-critical px-3 py-2 text-xs text-critical-soft">{error}</div>}
      {result && (
        <div className="mt-3 space-y-3">
          <pre className="overflow-x-auto rounded bg-paper px-3 py-2 text-xs font-mono text-ink-2">{result.sql}</pre>
          {result.explanation && <p className="text-xs text-ink-2">{result.explanation}</p>}
          <div className="overflow-x-auto rounded border border-rule">
            <table className="w-full text-xs">
              <thead className="bg-paper text-mute">
                <tr>
                  {result.columns.map((c: string) => (
                    <th key={c} className="px-2 py-1 text-left font-mono uppercase tracking-wide">{c}</th>
                  ))}
                </tr>
              </thead>
              <tbody>
                {result.rows.length === 0 ? (
                  <tr><td colSpan={result.columns.length} className="px-2 py-2 text-center text-mute"><T id="cockpit.sqlNoRows" /></td></tr>
                ) : result.rows.map((r: Record<string, unknown>, i: number) => (
                  <tr key={i} className="border-t border-rule">
                    {result.columns.map((c: string) => (
                      <td key={c} className="px-2 py-1 text-ink">{String(r[c] ?? '')}</td>
                    ))}
                  </tr>
                ))}
              </tbody>
            </table>
            <div className="border-t border-rule bg-paper px-2 py-1 text-xs font-mono text-mute">{result.rowCount} row{result.rowCount === 1 ? '' : 's'}</div>
          </div>
        </div>
      )}
    </section>
  );
}