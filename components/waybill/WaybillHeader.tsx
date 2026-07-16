import React from 'react';
import Link from 'next/link';
import type { SecondaryLocale } from '@/server/locale';
import type { WaybillRow } from '@/waybill/queries';
import { formatDateServer, formatMoneyServer } from '@/components/i18n/formattersServer';
import { T } from '@/components/i18n/TServer';

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
  const originEmoji = wb.origin === 'so' ? '🛒' : '📦';
  const localeSafe: SecondaryLocale = locale ?? 'th';
  const formattedAmount = await formatMoneyServer(wb.total_amount, localeSafe, wb.currency);
  const formattedCreated = await formatDateServer(wb.created_at, localeSafe);

  return (
    <header className="relative overflow-hidden rounded-3xl border border-slate-700/70 bg-gradient-to-br from-slate-900/80 via-indigo-950/40 to-slate-950/80 p-5 lg:p-6 shadow-2xl shadow-indigo-950/40">
      <div className={'pointer-events-none absolute inset-0 bg-gradient-to-br opacity-60 ' + statusTone.ring} aria-hidden />
      <div className="pointer-events-none absolute -top-24 -right-24 h-72 w-72 rounded-full bg-cyan-500/10 blur-3xl" aria-hidden />
      <div className="pointer-events-none absolute -bottom-24 -left-24 h-72 w-72 rounded-full bg-indigo-500/10 blur-3xl" aria-hidden />

      <div className="relative flex flex-wrap items-start justify-between gap-4">
        <div className="space-y-1.5">
          <div className="flex flex-wrap items-center gap-2">
            <span className="inline-flex items-center gap-1.5 rounded-full border border-slate-700 bg-slate-900/70 px-3 py-1 text-xs font-mono uppercase tracking-widest text-slate-300">
              <span aria-hidden>{originEmoji}</span>
              <a className="text-cyan-300 hover:text-cyan-200 underline-offset-2 hover:underline" href={originHref}>
                {originLabel}
              </a>
            </span>
            <span className={'inline-flex items-center gap-2 rounded-full border px-3 py-1 text-xs font-bold uppercase tracking-widest ' + statusTone.chip}>
              <span className={'h-2 w-2 rounded-full ' + statusTone.dot} aria-hidden />
              {statusTone.label}
            </span>
          </div>
          <h1 className="font-mono text-3xl font-bold tracking-tight text-white sm:text-4xl">
            {wb.id}
          </h1>
          <p className="text-lg font-semibold text-slate-200 flex items-center gap-2">
            <span>{wb.vendor_name ?? '—'}</span>
            {wb.created_to && (
              <>
                <span className="text-slate-500 font-normal">→</span>
                <span>{wb.created_to}</span>
              </>
            )}
          </p>
          <div className="flex flex-wrap items-center gap-x-4 gap-y-1 text-sm text-slate-400">
            <span className="inline-flex items-center gap-1.5">
              <span aria-hidden>👤</span>
              <span>{actor.fullname ?? `User ${actor.id}`}</span>
            </span>
            <span className="text-slate-700">•</span>
            <span className="inline-flex items-center gap-1.5 font-mono text-xs uppercase tracking-wider">
              <span aria-hidden>🕐</span>
              <span>
                <T id="waybill.headerCreated" locale={localeSafe} />{' '}
                <span className="text-slate-200">{formattedCreated}</span>
              </span>
            </span>
            {activeActorName && !isRejected && wb.status === 'open' && (
              <>
                <span className="text-slate-700">•</span>
                <span className="inline-flex items-center gap-1.5">
                  <span aria-hidden>⏳</span>
                  <span>
                    Waiting on{' '}
                    <span className="font-mono font-bold text-cyan-300">
                      {activeActorName}
                    </span>
                  </span>
                </span>
              </>
            )}
            <span className="text-slate-700">•</span>
            <Link href="/inbox" className="text-cyan-300 hover:text-cyan-200 underline-offset-2 hover:underline">
              📥 Inbox
            </Link>
          </div>
        </div>

        <div className="text-right">
          <div className="text-xs font-mono uppercase tracking-widest text-slate-400">
            <T id="waybill.headerTotalAmount" locale={localeSafe} />
          </div>
          <div className="mt-0.5 font-mono text-3xl font-bold text-emerald-300 sm:text-4xl">
            {formattedAmount}
          </div>
          <div className="mt-0.5 text-xs uppercase tracking-widest text-slate-500">
            {wb.currency}
          </div>
        </div>
      </div>

      <div className="relative mt-4">
        <div className="flex items-baseline justify-between text-xs font-mono uppercase tracking-widest text-slate-400">
          <span><T id="waybill.headerPipeline" locale={localeSafe} /></span>
          <span className="text-slate-200">
            step <span className="font-bold text-cyan-300">
              {wb.status === 'completed'
                ? stepsTotal
                : Math.max(0, stepsDone + (wb.status === 'open' ? 1 : 0))}
            </span> of {stepsTotal} · {progressPct}%
          </span>
        </div>

        <div className="mt-1.5 h-2 overflow-hidden rounded-full border border-slate-700/60 bg-slate-950/80 shadow-inner">
          <div
            className={
              'h-full rounded-full transition-[width] duration-700 ease-out ' +
              (isRejected
                ? 'bg-gradient-to-r from-rose-500 to-rose-400'
                : wb.status === 'completed'
                ? 'bg-gradient-to-r from-emerald-500 to-cyan-400'
                : 'bg-gradient-to-r from-cyan-400 via-indigo-400 to-purple-400')
            }
            style={{ width: `${progressPct}%` }}
            aria-hidden
          />
        </div>
      </div>
    </header>
  );
}
