'use client';

import { useState } from 'react';

export interface Nudge {
  waybill_id: string;
  stage: string;
  hint: string;
  sent_at: string;
}

interface Props {
  initial: Nudge[];
  lang: 'en' | 'th' | 'de';
}

export function NudgesPanel({ initial, lang }: Props) {
  const [nudges, setNudges] = useState<Nudge[]>(initial);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string | null>(null);

  const onGenerate = async () => {
    setBusy(true);
    setMessage(null);
    try {
      const res = await fetch('/api/waybill/nudges', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ lang }),
      });
      const json = await res.json();
      if (!res.ok || !json.ok) {
        setMessage(`Error: ${json.error ?? `HTTP ${res.status}`}`);
      } else {
        const sent = Array.isArray(json.sent) ? json.sent : [];
        setMessage(sent.length === 0 ? 'No stale queues — nothing to nudge.' : `Generated ${sent.length} new nudge${sent.length === 1 ? '' : 's'}.`);
        const refresh = await fetch('/api/waybill/nudges?limit=20');
        const j2 = await refresh.json();
        if (refresh.ok && j2.ok) setNudges(Array.isArray(j2.items) ? j2.items : []);
      }
    } catch (e: any) {
      setMessage(`Error: ${e?.message ?? String(e)}`);
    } finally {
      setBusy(false);
    }
  };

  return (
    <section>
      <div className="mb-4 flex items-center gap-3">
        <h2 className="text-xs font-mono uppercase tracking-widest text-slate-500">Recent nudges</h2>
        <button
          type="button"
          onClick={onGenerate}
          disabled={busy}
          className="ml-auto rounded-lg border border-cyan-500/40 bg-cyan-500/10 px-3 py-1.5 text-xs font-mono text-cyan-200 hover:bg-cyan-500/20 disabled:opacity-50"
        >
          {busy ? 'Generating…' : 'Generate fresh nudges now'}
        </button>
      </div>
      {message && <div className="mb-3 rounded border border-slate-800 bg-slate-950 px-3 py-2 text-xs text-slate-300">{message}</div>}
      {nudges.length === 0 ? (
        <div className="rounded-2xl border border-dashed border-slate-800 bg-slate-950/40 p-6 text-center text-sm text-slate-500">
          No nudges yet. Click "Generate fresh nudges now" to scan for idle waybills.
        </div>
      ) : (
        <ul className="space-y-2">
          {nudges.map((n, i) => (
            <li key={`${n.waybill_id}-${n.stage}-${i}`} className="rounded-2xl border border-slate-800 bg-slate-950/60 p-4">
              <div className="mb-1 flex items-center gap-2 text-xs font-mono text-slate-500">
                <a href={`/waybill/${n.waybill_id}`} className="text-cyan-300 hover:underline">{n.waybill_id}</a>
                <span>·</span>
                <span>{n.stage}</span>
                <span className="ml-auto">{new Date(n.sent_at).toISOString().slice(0, 16).replace('T', ' ')}</span>
              </div>
              <p className="text-sm text-slate-200">{n.hint}</p>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}