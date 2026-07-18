import React from 'react';
import Link from 'next/link';
import type { SecondaryLocale } from '@/server/locale';
import type { WaybillRow } from '@/waybill/queries';
import { formatDateServer, formatMoneyServer } from '@/components/i18n/formattersServer';
import { T } from '@/components/i18n/TServer';
import { Clock3, Hourglass, Inbox, PackageCheck, ShoppingCart, UserRound } from 'lucide-react';

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
  actor: { id: number; fullname: string | null };
  isRejected: boolean;
  statusTone: StatusTone;
  stepsDone: number;
  stepsTotal: number;
  progressPct: number;
  activeActorName: string | null;
  locale?: SecondaryLocale;
}

export async function WaybillHeader({
  wb,
  originLabel,
  originHref,
  actor,
  isRejected,
  statusTone,
  stepsDone,
  stepsTotal,
  progressPct,
  activeActorName,
  locale,
}: Props) {
  const localeSafe: SecondaryLocale = locale ?? 'th';
  const formattedAmount = await formatMoneyServer(wb.total_amount, localeSafe, wb.currency);
  const formattedCreated = await formatDateServer(wb.created_at, localeSafe);

  return (
    <header className="panel-elevated relative overflow-hidden p-5 lg:p-6">
      <div className={'pointer-events-none absolute inset-0 opacity-30 ' + statusTone.ring} aria-hidden />
      <div className="pointer-events-none absolute -top-24 -right-24 h-72 w-72 rounded-full bg-info blur-3xl" aria-hidden />
      <div className="pointer-events-none absolute -bottom-24 -left-24 h-72 w-72 rounded-full bg-accent blur-3xl" aria-hidden />

      <div className="relative flex flex-wrap items-start justify-between gap-4">
        <div className="space-y-1.5">
          <div className="flex flex-wrap items-center gap-2">
            <span className="inline-flex items-center gap-1.5 rounded-full border border-rule bg-paper-2/70 px-3 py-1 text-xs font-mono uppercase tracking-widest text-ink-2">
              {wb.origin === 'so' ? <ShoppingCart size={13} aria-hidden /> : <PackageCheck size={13} aria-hidden />}
              <a className="text-info hover:text-info-soft underline-offset-2 hover:underline" href={originHref}>
                {originLabel}
              </a>
            </span>
            <span className={'inline-flex items-center gap-2 rounded-full border px-3 py-1 text-xs font-bold uppercase tracking-widest ' + statusTone.chip}>
              <span className={'h-2 w-2 rounded-full ' + statusTone.dot} aria-hidden />
              {statusTone.label}
            </span>
          </div>
          <h1 className="font-mono text-3xl font-bold tracking-tight text-ink sm:text-4xl">
            {wb.id}
          </h1>
          <p className="text-lg font-semibold text-ink flex items-center gap-2">
            <span>{wb.vendor_name ?? '—'}</span>
            {wb.created_to && (
              <>
                <span className="text-mute font-normal">→</span>
                <span>{wb.created_to}</span>
              </>
            )}
          </p>
          <div className="flex flex-wrap items-center gap-x-4 gap-y-1 text-sm text-ink-2">
            <span className="inline-flex items-center gap-1.5">
              <UserRound size={14} aria-hidden />
              <span>{actor.fullname ?? `User ${actor.id}`}</span>
            </span>
            <span className="text-mute">•</span>
            <span className="inline-flex items-center gap-1.5 font-mono text-xs uppercase tracking-wider">
              <Clock3 size={14} aria-hidden />
              <span>
                <T id="waybill.headerCreated" locale={localeSafe} />{' '}
                <span className="text-ink">{formattedCreated}</span>
              </span>
            </span>
            {activeActorName && !isRejected && wb.status === 'open' && (
              <>
                <span className="text-mute">•</span>
                <span className="inline-flex items-center gap-1.5">
                  <Hourglass size={14} aria-hidden />
                  <span>
                    Waiting on{' '}
                    <span className="font-mono font-bold text-info">
                      {activeActorName}
                    </span>
                  </span>
                </span>
              </>
            )}
            <span className="text-mute">•</span>
            <Link href="/inbox" className="inline-flex items-center gap-1.5 text-info hover:text-info-strong underline-offset-2 hover:underline">
              <Inbox size={14} aria-hidden /> Inbox
            </Link>
          </div>
        </div>

        <div className="text-right">
          <div className="text-xs font-mono uppercase tracking-widest text-ink-2">
            <T id="waybill.headerTotalAmount" locale={localeSafe} />
          </div>
          <div className="mt-0.5 font-mono text-3xl font-bold text-positive sm:text-4xl">
            {formattedAmount}
          </div>
          <div className="mt-0.5 text-xs uppercase tracking-widest text-mute">
            {wb.currency}
          </div>
        </div>
      </div>

      <div className="relative mt-4">
        <div className="flex items-baseline justify-between text-xs font-mono uppercase tracking-widest text-ink-2">
          <span><T id="waybill.headerPipeline" locale={localeSafe} /></span>
          <span className="text-ink">
            step <span className="font-bold text-info">
              {wb.status === 'completed'
                ? stepsTotal
                : Math.max(0, stepsDone + (wb.status === 'open' ? 1 : 0))}
            </span> of {stepsTotal} · {progressPct}%
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
