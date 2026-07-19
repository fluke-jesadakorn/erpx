import React from 'react';
import type { SecondaryLocale } from '@/server/locale';
import type { WaybillRow } from '@/waybill/queries';
import { formatDateServer, formatMoneyServer } from '@/components/i18n/formattersServer';
import { T } from '@/components/i18n/TServer';
import { Clock3, PackageCheck, ShoppingCart, UserRound } from 'lucide-react';

type StatusTone = {
  ring: string;
  chip: string;
  dot: string;
  label: React.ReactNode;
};

interface Props {
  wb: WaybillRow;
  originLabel: React.ReactNode;
  originHref: string;
  actor?: { id: number; fullname: string | null };
  submitterName?: string | null;
  vendorName?: string | null;
  totalAmount?: string | number | null;
  actions?: React.ReactNode;
  isRejected: boolean;
  statusTone: StatusTone;
  stepsDone: number;
  stepsTotal: number;
  progressPct: number;
  activeActorName?: string | null;
  locale?: SecondaryLocale;
}

export async function WaybillHeader({
  wb,
  originLabel,
  originHref,
  actor,
  submitterName,
  vendorName,
  totalAmount,
  actions,
  isRejected,
  statusTone,
  stepsDone,
  stepsTotal,
  progressPct,
  locale,
}: Props) {
  const localeSafe: SecondaryLocale = locale ?? 'th';
  const formattedAmount = await formatMoneyServer(totalAmount ?? wb.total_amount, localeSafe, wb.currency);
  const formattedCreated = await formatDateServer(wb.created_at, localeSafe);

  return (
    <header className="panel-elevated relative overflow-hidden p-5">
      <div className={'pointer-events-none absolute inset-0 opacity-30 ' + statusTone.ring} aria-hidden />
      <div className="pointer-events-none absolute -top-24 -right-24 h-72 w-72 rounded-full bg-info blur-3xl" aria-hidden />
      <div className="pointer-events-none absolute -bottom-24 -left-24 h-72 w-72 rounded-full bg-accent blur-3xl" aria-hidden />
      <div className="pointer-events-none absolute inset-0 bg-paper/75" aria-hidden />

      <div className="relative flex flex-wrap items-start justify-between gap-4">
        <div className="min-w-0 space-y-2 sm:w-auto">
          <div className="flex flex-wrap items-center gap-2">
            <span className="inline-flex items-center gap-1.5 rounded-full border border-rule bg-paper-2/70 px-3 py-1 text-xs font-mono uppercase tracking-widest text-ink-2">
              {wb.origin === 'so' ? <ShoppingCart size={13} aria-hidden /> : <PackageCheck size={13} aria-hidden />}
              <a className="text-info-strong hover:text-info-strong underline-offset-2 hover:underline" href={originHref}>
                {originLabel}
              </a>
            </span>
            <span className={'inline-flex items-center gap-2 rounded-full border px-3 py-1 text-xs font-bold uppercase tracking-widest ' + statusTone.chip}>
              <span className={'h-2 w-2 rounded-full ' + statusTone.dot} aria-hidden />
              {statusTone.label}
            </span>
          </div>
          <h1 className="whitespace-nowrap font-mono text-[clamp(1.55rem,8vw,2.25rem)] font-bold tracking-tight text-ink sm:text-4xl">
            {wb.id}
          </h1>
          <p className="text-lg font-semibold text-ink">{vendorName ?? wb.vendor_name ?? '—'}</p>
          <div className="flex flex-wrap items-center gap-x-4 gap-y-2 text-sm text-ink-2">
            <span className="inline-flex items-center gap-1.5">
              <UserRound size={14} aria-hidden />
              <span>{submitterName ?? actor?.fullname ?? (wb.submitter_id ? `User ${wb.submitter_id}` : '—')}</span>
            </span>
            <span className="inline-flex items-center gap-1.5 text-sm">
              <Clock3 size={14} aria-hidden />
              <span>
                <T id="waybill.headerCreated" locale={localeSafe} variant="compact" />{' '}
                <span className="font-mono text-xs text-ink">{formattedCreated}</span>
              </span>
            </span>
          </div>
        </div>

        <div className="flex w-full flex-col items-start gap-3 text-left sm:w-auto sm:shrink-0 sm:items-end sm:text-right">
          {actions}
          <div className="text-xs font-medium text-ink-2">
            <T id="waybill.headerTotalAmount" locale={localeSafe} variant="compact" />
          </div>
          <div className="whitespace-nowrap font-mono text-3xl font-bold text-positive-strong sm:text-4xl">
            {formattedAmount}
          </div>
        </div>
      </div>

      <div className="relative mt-5 border-t border-rule/50 pt-4">
        <div className="flex flex-col gap-1 text-xs text-ink-2 sm:flex-row sm:items-baseline sm:justify-between sm:gap-4">
          <span><T id="waybill.headerPipeline" locale={localeSafe} variant="compact" /></span>
          <span className="text-ink">
            <T
              id="waybill.headerStepOf"
              locale={localeSafe}
              variant="compact"
              values={{
                cur: wb.status === 'completed'
                  ? stepsTotal
                  : Math.max(0, stepsDone + (wb.status === 'open' ? 1 : 0)),
                total: stepsTotal,
                pct: progressPct,
              }}
            />
          </span>
        </div>

        <div className="mt-1.5 h-2 overflow-hidden rounded-full border border-rule/60 bg-paper-2/80 shadow-inner">
          <div
            className={
              'h-full rounded-full transition-[width] duration-700 ease-out ' +
              (isRejected
                ? 'bg-critical'
                : wb.status === 'completed'
                ? 'bg-positive'
                : 'bg-info')
            }
            style={{ width: `${progressPct}%` }}
            aria-hidden
          />
        </div>
      </div>
    </header>
  );
}
