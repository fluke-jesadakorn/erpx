import React from 'react';
import { Check, Circle, LockKeyhole, Zap } from 'lucide-react';
import { Badge } from '@/components/ui';
import { T } from '@/components/i18n/T';
import {
  computePipState,
  domainForOrigin,
  pipsForDomain,
  pipIndex,
  type WaybillDomain,
} from '@/waybill/derive';
import type { WaybillRow } from '@/waybill/queries';

interface Props {
  wb: WaybillRow;
  currentStage: string;
  status: string;
  canAct?: boolean;
  indicators?: React.ReactNode;
}

const STATE_TONE = {
  passed: 'positive',
  active: 'accent',
  pending: 'neutral',
  rejected: 'critical',
  skipped: 'neutral',
} as const;

function badgeKey(state: ReturnType<typeof computePipState>, canAct: boolean): string {
  if (state === 'passed') return 'waybill.pip.done';
  if (state === 'active') return canAct ? 'waybill.pip.yourTurn' : 'waybill.pip.currentStage';
  if (state === 'rejected') return 'waybill.pip.stop';
  if (state === 'skipped') return 'waybill.pip.skip';
  return 'waybill.pip.pending';
}

export function WaybillRail({ wb, currentStage, status, canAct = false, indicators }: Props) {
  const domain: WaybillDomain = domainForOrigin(wb.origin);
  const pips = pipsForDomain(domain);
  const currentIndex = pipIndex(domain, currentStage);
  const completedCount = status === 'completed'
    ? pips.length
    : Math.max(0, currentIndex);

  return (
    <section aria-label="Waybill progress" className="panel space-y-5 p-4 sm:p-5">
      <header className="flex flex-wrap items-center justify-between gap-3">
        <h2 className="text-base font-semibold text-ink">
          <T
            id={domain === 'procurement' ? 'waybill.timeline.procurementPipeline' : 'waybill.timeline.expensePipeline'}
            variant="stacked"
            primaryClassName="block text-base font-semibold text-ink"
            secondaryClassName="mt-1 block text-xs font-normal text-mute"
          />
        </h2>
        <div className="flex flex-wrap items-center gap-2">
          {indicators}
          <Badge tone={status === 'rejected' ? 'critical' : status === 'completed' ? 'positive' : 'neutral'} size="md">
            {status === 'completed' ? (
              <T id="waybill.status.completed" />
            ) : status === 'rejected' ? (
              <T id="waybill.status.rejected" />
            ) : (
              <>{completedCount + (currentIndex >= 0 && status === 'open' ? 1 : 0)} / {pips.length}</>
            )}
          </Badge>
        </div>
      </header>

      <ol className="flex w-full max-w-full snap-x snap-mandatory gap-2 overflow-x-auto pb-2 md:grid md:grid-cols-7 md:gap-0 md:overflow-visible">
        {pips.map((pip, index) => {
          const state = computePipState(pip, index, currentIndex, currentStage, status);
          const isCurrent = state === 'active';
          const isLast = index === pips.length - 1;
          return (
            <li key={pip.key} className="flex w-[8.25rem] min-w-[8.25rem] snap-start flex-none items-start gap-2 md:w-auto md:min-w-0">
              <div className="min-w-0 flex-1 text-center">
                <div className="flex items-center">
                  <span
                    className={[
                      'mx-auto grid h-9 w-9 shrink-0 place-items-center rounded-full border-2 transition',
                      state === 'passed' ? 'border-positive bg-positive text-paper' :
                        state === 'active' ? 'border-info bg-info text-paper shadow-[0_0_0_4px_var(--info-soft)]' :
                          state === 'rejected' ? 'border-critical bg-critical-soft text-critical' :
                            'border-rule-strong bg-paper-2 text-mute',
                      isCurrent ? 'animate-pulse' : '',
                    ].join(' ')}
                    aria-current={isCurrent ? 'step' : undefined}
                  >
                    {state === 'passed' ? <Check size={16} strokeWidth={3} aria-hidden /> :
                      state === 'active' ? <Zap size={15} strokeWidth={2.5} aria-hidden /> :
                        state === 'pending' ? <Circle size={13} aria-hidden /> :
                          <LockKeyhole size={13} aria-hidden />}
                  </span>
                  {!isLast && (
                    <span
                      aria-hidden
                      className={[
                        'ml-2 h-0.5 min-w-4 flex-1',
                        index < currentIndex ? 'bg-positive' : 'border-t border-dashed border-rule-strong',
                      ].join(' ')}
                    />
                  )}
                </div>
                <p className={[
                  'mt-3 min-h-12 break-words px-1 text-xs font-semibold leading-tight',
                  state === 'active' ? 'text-info-strong' : state === 'passed' ? 'text-positive-strong' : 'text-ink-2',
                ].join(' ')}>
                  <T
                    id={pip.label}
                    variant="stacked"
                    primaryClassName="block font-semibold leading-tight"
                    secondaryClassName="mt-1 block text-[11px] font-normal leading-tight text-mute"
                  />
                </p>
                {(isCurrent || state === 'rejected') && (
                  <span className="mt-1 inline-flex max-w-full">
                    <Badge tone={STATE_TONE[state]} size="sm">
                      <T id={badgeKey(state, canAct)} variant="compact" />
                    </Badge>
                  </span>
                )}
              </div>
            </li>
          );
        })}
      </ol>

    </section>
  );
}

export default WaybillRail;
