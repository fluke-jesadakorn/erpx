'use client';
import { useState } from 'react';
import type { ChartSpec } from './chartContract';
import { useT } from '@/components/i18n/useT';
import { T } from '@/components/i18n/T';
import type { BilingualText } from '@folio-lib/i18n/types';
import { useSecondaryLocale } from '@/components/i18n/SecondaryLocaleProvider';
import chatDict from '@folio-lib/i18n/chat';

const KEY = 'folio.pinned_charts';

export function PinToCockpitButton({ spec, tileId }: { spec: ChartSpec; tileId: string }) {
  const t = useT(chatDict);
  const locale = useSecondaryLocale();
  const [busy, setBusy] = useState(false);
  const [pinned, setPinned] = useState(false);

  const pick = (k: string): BilingualText => t(k);
  const plain = (k: string) => {
    const b = pick(k);
    return b[locale] ?? b.en;
  };

  function pin() {
    setBusy(true);
    try {
      const cur = JSON.parse(localStorage.getItem(KEY) || '[]') as Array<any>;
      const id = Math.random().toString(36).slice(2);
      const entry = { id, tileId, spec, pinnedAt: new Date().toISOString() };
      const dedupe = cur.filter((c) => JSON.stringify(c.spec) !== JSON.stringify(spec));
      const next = [entry, ...dedupe].slice(0, 6);
      localStorage.setItem(KEY, JSON.stringify(next));
      setPinned(true);
      setTimeout(() => setPinned(false), 1200);
    } catch {}
    setBusy(false);
  }

  return (
    <button
      type="button"
      onClick={pin}
      disabled={busy}
      className="text-xs px-2 py-1 rounded bg-amber-500/15 text-amber-200 border border-amber-500/30 hover:bg-amber-500/25 font-mono"
      title={plain('chat.pin.title')}
    >
      <T value={pinned ? pick('chat.pin.pinned') : pick('chat.pin.action')} />
    </button>
  );
}