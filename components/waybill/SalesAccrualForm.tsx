import React from 'react';
import { fmtDate, fmtTs } from './ui';
import { GlVisibilityGate } from './GlVisibilityGate';
import {
  confirmSalesGlAction,
  postSalesGlAccrualAction,
  postSalesGlSettlementAction,
  postSalesGlVatAction,
} from '@/app/actions/sales';
import type { ProcurementJournalStepView, SalesJournalView } from '@/waybill/queries';
import { LinesTable, summarizeLines } from './GlLinesTable';
import { T } from '@/components/i18n/TServer';

type Locale = 'th' | 'de';

export function SalesAccrualForm({
  waybillId,
  journal,
  actorCanSeeLines,
  canPostVat,
  canPostAccrual,
  canPostSettlement,
  canConfirm,
  locale,
}: {
  waybillId: string;
  journal: SalesJournalView;
  actorCanSeeLines: boolean;
  canPostVat: boolean;
  canPostAccrual: boolean;
  canPostSettlement: boolean;
  canConfirm: boolean;
  locale: Locale;
}) {
  const stepVatPosted = !!journal.vat.posted;
  const stepAccrualPosted = !!journal.accrual.posted;
  const stepSettlementPosted = !!journal.settlement.posted;

  return (
    <div className="space-y-3">
      <SalesStep
        waybillId={waybillId}
        step={journal.vat}
        stepNo={1}
        emoji="🧾"
        title={<T id="waybill.gl.vatTitle" locale={locale} />}
        subtitle={
          <T id="waybill.gl.vatSub" locale={locale} />
        }
        tone="cyan"
        actorCanSeeLines={actorCanSeeLines}
        canPost={canPostVat}
        canConfirm={canConfirm}
        postAction={postSalesGlVatAction}
        postLabel={<T id="waybill.gl.postVat" locale={locale} />}
        locale={locale}
      />
      <SalesStep
        waybillId={waybillId}
        step={journal.accrual}
        stepNo={2}
        emoji="📒"
        title={<T id="waybill.gl.accrualTitle" locale={locale} />}
        subtitle={
          <T id="waybill.gl.accrualSub" locale={locale} />
        }
        tone="emerald"
        actorCanSeeLines={actorCanSeeLines}
        canPost={canPostAccrual}
        canConfirm={canConfirm}
        postAction={postSalesGlAccrualAction}
        postLabel={<T id="waybill.gl.postAccrualSales" locale={locale} />}
        locked={!stepVatPosted}
        blockedReason={
          !stepVatPosted ? (
            <T id="waybill.gl.lockedUntil1" locale={locale} />
          ) : null
        }
        locale={locale}
      />
      <SalesStep
        waybillId={waybillId}
        step={journal.settlement}
        stepNo={3}
        emoji="💳"
        title={<T id="waybill.gl.settlementTitle" locale={locale} />}
        subtitle={
          <T id="waybill.gl.settlementSub" locale={locale} />
        }
        tone="amber"
        actorCanSeeLines={actorCanSeeLines}
        canPost={canPostSettlement}
        canConfirm={canConfirm}
        postAction={postSalesGlSettlementAction}
        postLabel={<T id="waybill.gl.postSettlementSales" locale={locale} />}
        locked={!stepAccrualPosted}
        blockedReason={
          !stepAccrualPosted ? (
            <T id="waybill.gl.lockedUntil2" locale={locale} />
          ) : null
        }
        locale={locale}
      />
      <p className="font-mono text-sm uppercase tracking-widest text-ink-2">
        <T id="waybill.gl.eachLockedUntilPrev" locale={locale} />
        {!stepSettlementPosted && (
          <>
            {' · '}
            <span className="text-ink-2/80">
              <T id="waybill.gl.settlementAtSoPaid" locale={locale} />
            </span>
          </>
        )}
      </p>
    </div>
  );
}

function SalesStep({
  waybillId,
  step,
  stepNo,
  emoji,
  title,
  subtitle,
  tone,
  actorCanSeeLines,
  canPost,
  canConfirm,
  postAction,
  postLabel,
  blockedReason,
  locked,
  locale,
}: {
  waybillId: string;
  step: ProcurementJournalStepView;
  stepNo: 1 | 2 | 3;
  emoji: string;
  title: React.ReactNode;
  subtitle: React.ReactNode;
  tone: 'cyan' | 'emerald' | 'amber';
  actorCanSeeLines: boolean;
  canPost: boolean;
  canConfirm: boolean;
  postAction: (formData: FormData) => Promise<void>;
  postLabel: React.ReactNode;
  blockedReason?: React.ReactNode;
  locked?: boolean;
  locale: Locale;
}) {
  const { draft, posted, posted_event, confirmed_event } = step;
  const liveLines = draft?.lines ?? posted?.lines ?? [];
  const totals = summarizeLines(liveLines);

  const actions: React.ReactNode[] = [];
  if (canPost && draft && !posted) {
    actions.push(
      <form key="post" action={postAction}>
        <input type="hidden" name="waybillId" value={waybillId} />
        <input type="hidden" name="journalId" value={String(draft.journal_id)} />
        <button
          type="submit"
          className={
            'inline-flex items-center gap-1.5 rounded-lg px-3 py-1.5 text-sm font-bold shadow transition-colors ' +
            (tone === 'amber'
              ? 'bg-caution text-paper-2 hover:bg-caution-strong'
              : 'bg-positive text-paper-2 hover:bg-positive-strong')
          }
        >
          <span aria-hidden>✓</span>
          <span>{postLabel}</span>
        </button>
      </form>,
    );
  }
  if (canConfirm && posted && !confirmed_event) {
    actions.push(
      <form key="confirm" action={confirmSalesGlAction}>
        <input type="hidden" name="waybillId" value={waybillId} />
        <input type="hidden" name="step" value={stepNo === 3 ? 'settlement' : stepNo === 2 ? 'accrual' : 'vat'} />
        <button
          type="submit"
          className="inline-flex items-center gap-1.5 rounded-lg bg-caution px-3 py-1.5 text-sm font-bold text-paper-2 hover:bg-caution-strong transition-colors"
        >
          <span aria-hidden>✓</span>
          <span><T id="waybill.gl.confirmGl" locale={locale} /></span>
        </button>
      </form>,
    );
  }

  const meta: React.ReactNode[] = [];
  if (posted) {
    meta.push(
      <p key="posted-by" className="rounded-md border border-positive/40 bg-positive-soft p-2 font-mono text-positive-strong">
        <span className="text-positive">posted_by:</span>{' '}
        {posted.finalized_by_name ?? '—'}{' '}
        <span className="text-ink-2">#{posted.finalized_by ?? '—'}</span>
        {posted.finalized_at && (
          <>
            <span className="mx-1 text-mute">·</span>
            <span className="text-positive/80">{fmtTs(posted.finalized_at, locale === 'de' ? 'en' as const : locale)}</span>
          </>
        )}
      </p>,
    );
  }
  if (posted_event) {
    meta.push(
      <p key="posted-event" className="rounded-md border border-positive/40 bg-positive-soft p-2 font-mono text-positive-strong">
        <span className="text-positive">posted_event:</span>{' '}
        {posted_event.actor_name ?? '—'}{' '}
        <span className="text-ink-2">#{posted_event.actor_id ?? '—'}</span>
        <span className="mx-1 text-mute">·</span>
        <span className="text-positive/80">{fmtTs(posted_event.occurred_at, locale === 'de' ? 'en' as const : locale)}</span>
      </p>,
    );
  }
  if (confirmed_event) {
    meta.push(
      <p key="confirmed-event" className="rounded-md border border-info/40 bg-info-soft p-2 font-mono text-info-strong sm:col-span-2">
        <span className="text-info">confirmed_event:</span>{' '}
        {confirmed_event.actor_name ?? '—'}{' '}
        <span className="text-ink-2">#{confirmed_event.actor_id ?? '—'}</span>
        <span className="mx-1 text-mute">·</span>
        <span className="text-info/80">{fmtTs(confirmed_event.occurred_at, locale === 'de' ? 'en' as const : locale)}</span>
      </p>,
    );
  }

  return (
    <section className={'space-y-3 rounded-md border ' + (tone === 'cyan'
      ? 'border-info/40 bg-info-soft/40'
      : tone === 'amber'
        ? 'border-caution/40 bg-caution-soft/40'
        : 'border-positive/40 bg-positive-soft/40')}>
      <header className="flex flex-wrap items-baseline justify-between gap-3 px-4 py-3">
        <div className="flex items-center gap-3">
          <span aria-hidden className="grid h-9 w-9 place-items-center rounded-md bg-info-soft text-info text-lg ring-1 ring-info/40">
            {emoji}
          </span>
          <div className="flex flex-col">
            <div className="flex items-center gap-2 text-base font-bold text-ink">
              <span className="bg-paper-2 border border-rule rounded-md border px-1.5 py-0.5 font-mono text-sm uppercase tracking-widest text-ink-2">
                <T id="waybill.gl.stepLabel" values={{ n: stepNo }} locale={locale} />
              </span>
              <span>{title}</span>
            </div>
            <span className="font-mono text-sm uppercase tracking-widest text-ink-2">{subtitle}</span>
          </div>
        </div>
        {actions.length > 0 && (
          <div className="flex flex-wrap items-center gap-2">{actions}</div>
        )}
      </header>

      <div className={'space-y-3 border-t border-rule px-4 py-3 ' + (locked ? 'pointer-events-none opacity-60' : '')}>
        {blockedReason && (
          <p className="bg-paper-2 border border-rule rounded-md border p-3 text-sm italic text-ink-2">{blockedReason}</p>
        )}

        {draft && (
          <section className="space-y-2 rounded-lg border border-info/40 bg-info-soft/30 p-3">
            <header className="flex flex-wrap items-baseline justify-between gap-2">
              <div className="flex items-center gap-2">
                <span className="rounded-md border border-info/50 bg-info/15 px-2 py-0.5 text-sm font-mono font-bold uppercase text-info-strong">
                  📝 DRAFT
                </span>
                <span className="font-mono text-sm text-ink-2">JE #{draft.journal_id}</span>
              </div>
              <span className="font-mono text-sm text-ink-2">
                entry_date: <span className="text-info">{fmtDate(draft.entry_date)}</span>
              </span>
            </header>
            {draft.description && <p className="text-sm text-ink">{draft.description}</p>}
            <GlVisibilityGate
              actorCanSeeLines={actorCanSeeLines}
              totalDebit={totals.totalDebit}
              totalCredit={totals.totalCredit}
              balanced={totals.balanced}
              lineCount={draft.lines.length}
            >
              <LinesTable lines={draft.lines} locale={locale} />
            </GlVisibilityGate>
          </section>
        )}

        {posted && (
          <section className="space-y-2 rounded-lg border border-positive/40 bg-positive-soft/30 p-3">
            <header className="flex flex-wrap items-baseline justify-between gap-2">
              <div className="flex items-center gap-2">
                <span className="rounded-md border border-positive/50 bg-positive/15 px-2 py-0.5 text-sm font-mono font-bold uppercase text-positive-strong">
                  ✓ POSTED
                </span>
                <span className="font-mono text-sm text-ink-2">JE #{posted.journal_id}</span>
              </div>
              <span className="font-mono text-sm text-ink-2">
                entry_date: <span className="text-positive">{fmtDate(posted.entry_date)}</span>
              </span>
            </header>
            {posted.description && <p className="text-sm text-ink">{posted.description}</p>}
            <GlVisibilityGate
              actorCanSeeLines={actorCanSeeLines}
              totalDebit={totals.totalDebit}
              totalCredit={totals.totalCredit}
              balanced={totals.balanced}
              lineCount={posted.lines.length}
            >
              <LinesTable lines={posted.lines} locale={locale} />
            </GlVisibilityGate>
            {meta.length > 0 && (
              <div className="grid gap-2 text-xs sm:grid-cols-2">{meta}</div>
            )}
          </section>
        )}
      </div>
    </section>
  );
}
