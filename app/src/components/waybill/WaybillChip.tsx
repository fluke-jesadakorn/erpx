import React from 'react';
import type { WaybillDomain } from '@folio-lib/waybill/derive';
import { pipsForDomain, findPip } from '@folio-lib/waybill/derive';

interface Props {
  domain: WaybillDomain;
  currentStage: string;
  amountTHB?: number | null;
  lang?: 'en' | 'th';
  size?: 'sm' | 'md' | 'lg';
}

const STATE_STYLES = {
  passed:  'border-emerald-500/40 bg-emerald-500/10 text-emerald-300',
  active:  'border-cyan-500 bg-cyan-500/15 text-cyan-200',
  pending: 'border-slate-700 bg-slate-800/50 text-slate-400',
  rejected:'border-rose-500/40 bg-rose-500/10 text-rose-300',
  skipped: 'border-slate-800 bg-slate-900/40 text-slate-600',
};

export function WaybillChip({
  domain,
  currentStage,
  amountTHB = null,
  lang = 'en',
  size = 'md',
}: Props) {
  const pip = findPip(domain, currentStage);
  const pips = pipsForDomain(domain);
  const idx = pips.findIndex((p) => p.key === currentStage);
  const isRejected = currentStage === 'rejected';
  const isClosed = currentStage === 'disbursed' || isRejected;
  const needCeo =
    domain === 'expense' && typeof amountTHB === 'number' && amountTHB >= 200_000;

  const effectiveCount = pips.filter((p) => !(p.key === 'ceo_authorization' && !needCeo)).length;
  const totalActive = effectiveCount;

  let state: 'passed' | 'active' | 'pending' | 'rejected' | 'skipped' = 'pending';
  if (isRejected) state = 'rejected';
  else if (isClosed) state = 'passed';
  else if (currentStage === currentStage && idx >= 0) state = 'active';

  const sizeClass =
    size === 'sm' ? 'px-2 py-0.5 text-xs' :
    size === 'lg' ? 'px-3 py-1.5 text-sm' :
    'px-2.5 py-1 text-sm';

  return (
    <span
      className={`inline-flex items-center gap-1.5 rounded-full border font-mono font-bold uppercase tracking-wider ${STATE_STYLES[state]} ${sizeClass}`}
      title={pip ? `${lang === 'th' ? pip.th : pip.en} · Waybill · ใบส่งของ` : 'Waybill · ใบส่งของ'}
      aria-label={pip ? `${lang === 'th' ? pip.th : pip.en} · Waybill · ใบส่งของ` : 'Waybill · ใบส่งของ'}
    >
      <span aria-hidden>{pip?.emoji ?? '📦'}</span>
      <span>{pip ? (lang === 'th' ? pip.th : pip.en) : (isRejected ? 'Rejected' : currentStage)}</span>
      {!isClosed && idx >= 0 && (
        <span className="ml-1 opacity-60">
          {idx + 1}/{totalActive}
        </span>
      )}
    </span>
  );
}
