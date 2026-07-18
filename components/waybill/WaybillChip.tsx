import React from 'react';
import type { WaybillDomain } from '@/waybill/derive';
import { pipsForDomain, findPip } from '@/waybill/derive';
import { T } from '@/components/i18n/T';

interface Props {
  domain: WaybillDomain;
  currentStage: string;
  amountTHB?: number | null;
  lang?: 'en' | 'th';
  size?: 'sm' | 'md' | 'lg';
}

const STATE_STYLES = {
  passed:  'border-positive bg-positive text-positive',
  active:  'border-info bg-info text-paper',
  pending: 'border-rule bg-paper-2/50 text-ink-2',
  rejected:'border-critical bg-critical text-critical',
  skipped: 'border-rule bg-paper-2/40 text-mute',
};

export function WaybillChip({
  domain,
  currentStage,
  amountTHB = null,
  lang: _lang = 'en',
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
      title={pip ? pip.label : 'waybill.chip.waybillTitle'}
      aria-label={pip ? pip.label : 'waybill.chip.waybillTitle'}
    >
      <span aria-hidden>{pip?.emoji ?? '📦'}</span>
      <T id={pip?.label ?? (isRejected ? 'waybill.stage.rejected' : `waybill.stage.${currentStage}`)} />
      {!isClosed && idx >= 0 && (
        <span className="ml-1 opacity-60">
          {idx + 1}/{totalActive}
        </span>
      )}
    </span>
  );
}
