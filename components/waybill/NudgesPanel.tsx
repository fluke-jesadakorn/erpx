'use client';

import React, { useState } from 'react';
import { T } from '@/components/i18n/T';
import { Empty } from '@/components/ui/Empty';

interface Nudge {
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
        <h2 className="text-xs font-mono uppercase tracking-widest text-mute">
          <T id="waybill.nudges.title" />
        </h2>
        <button
          type="button"
          onClick={onGenerate}
          disabled={busy}
          className="ml-auto rounded-lg border border-info bg-info px-3 py-1.5 text-xs font-mono text-info-soft hover:bg-info disabled:opacity-50"
        >
          {busy ? <T id="waybill.export.generating" /> : <T id="waybill.nudges.generateBtn" />}
        </button>
      </div>
      {message && <div className="mb-3 rounded border border-rule bg-paper px-3 py-2 text-xs text-ink-2">{message}</div>}
      {nudges.length === 0 ? (
        <Empty
          title={<T id="waybill.nudges.noStaleQueues" />}
          body="Generate nudges to surface stale waybills."
        />
      ) : (
        <ul className="space-y-2">
          {nudges.map((n, i) => (
            <li key={`${n.waybill_id}-${n.stage}-${i}`} className="rounded-md border border-rule bg-paper-2/60 p-4">
              <div className="mb-1 flex items-center gap-2 text-xs font-mono text-mute">
                <a href={`/waybill/${n.waybill_id}`} className="text-info hover:underline">{n.waybill_id}</a>
                <span>·</span>
                <span>{n.stage}</span>
                <span className="ml-auto">{new Date(n.sent_at).toISOString().slice(0, 16).replace('T', ' ')}</span>
              </div>
              <p className="text-sm text-ink">{n.hint}</p>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
