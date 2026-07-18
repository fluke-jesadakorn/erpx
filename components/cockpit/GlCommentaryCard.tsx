'use client';

import { useState } from 'react';
import { T } from '@/components/i18n/T';

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
    <section className="rounded-md border border-accent/30 bg-accent-soft/20 p-5">
      <h3 className="mb-2 text-xs font-mono uppercase tracking-widest text-accent">
        <T id="cockpit.glCommentaryTitle" />
      </h3>
      <p className="mb-3 text-xs text-mute">
        <T id="cockpit.glCommentaryHint" />
      </p>
      <div className="flex gap-2">
        <input
          type="text"
          value={code}
          onChange={(e) => setCode(e.target.value)}
          placeholder="account code (e.g. 510200)"
          className="flex-1 rounded-lg border border-rule bg-paper px-3 py-2 text-sm font-mono text-ink placeholder-mute focus:border-accent focus:outline-none"
        />
        <button
          type="button"
          onClick={onAsk}
          disabled={busy || !code.trim()}
          className="rounded-lg border border-accent/40 bg-accent/10 px-4 py-1.5 text-xs font-mono text-accent hover:bg-accent/20 disabled:opacity-50"
        >
          {busy ? <T id="cockpit.asking" /> : <T id="cockpit.explain" />}
        </button>
      </div>
      {error && <div className="mt-3 rounded border border-critical bg-critical px-3 py-2 text-xs text-critical-soft">{error}</div>}
      {result && (
        <div className="mt-3 space-y-2">
          {result.accountName && (
            <div className="text-xs font-mono text-ink-2">
              {result.accountCode} · {result.accountName}{result.accountNameTh ? ` · ${result.accountNameTh}` : ''}
            </div>
          )}
          <p className="whitespace-pre-wrap text-sm leading-relaxed text-ink">{result.commentary}</p>
        </div>
      )}
    </section>
  );
}