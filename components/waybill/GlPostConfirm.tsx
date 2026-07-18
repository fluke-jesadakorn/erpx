import React from 'react';
import Link from 'next/link';
import { fmtDate, fmtTs } from './ui';
import { GlVisibilityGate } from './GlVisibilityGate';
import {
  confirmGlRecordedAction,
  finalApproveWaybillAction,
  recomputeExpenseDraftGlAction,
} from '@/app/actions/waybill';
import {
  confirmProcurementGlAction,
  postProcurementAccrualAction,
  postProcurementSettlementAction,
  saveProcurementAccrualAction,
} from '@/app/actions/procurement';
import type { ExpenseJournalView, ProcurementJournalStepView } from '@/waybill/queries';
import { LinesTable, summarizeLines } from './GlLinesTable';
import { T } from '@/components/i18n/TServer';

type Locale = 'th' | 'de';

export function ExpenseGlPostConfirm({
  waybillId,
  journal,
  canFinalApprove,
  canConfirmGl,
  isFinalApproval,
  isDisbursed,
  actorCanSeeLines,
  locale,
}: {
  waybillId: string;
  journal: ExpenseJournalView;
  canFinalApprove: boolean;
  canConfirmGl: boolean;
  isFinalApproval: boolean;
  isDisbursed: boolean;
  actorCanSeeLines: boolean;
  locale: Locale;
}) {
  const { draft, posted, posted_event, confirmed_event } = journal;
  const empty = !draft && !posted;
  const liveLines = draft?.lines ?? posted?.lines ?? [];
  const totals = summarizeLines(liveLines);

  const headerActions: React.ReactNode[] = [];
  if (draft && !posted) {
    headerActions.push(
      <form key="recompute" action={recomputeExpenseDraftGlAction}>
        <input type="hidden" name="waybillId" value={waybillId} />
        <button
          type="submit"
          className="rounded-lg border border-info bg-info px-3 py-1.5 text-sm font-mono text-info-soft hover:bg-info"
        >
          ⟳ <T id="waybill.gl.recomputeDraft" locale={locale} />
        </button>
      </form>,
    );
  }
  if (canFinalApprove && isFinalApproval) {
    headerActions.push(
      <form key="final" action={finalApproveWaybillAction}>
        <input type="hidden" name="waybillId" value={waybillId} />
        <button
          type="submit"
          className="inline-flex items-center gap-1.5 rounded-lg bg-positive px-3 py-1.5 text-sm font-bold text-paper hover:bg-positive-strong"
        >
          <span aria-hidden>✓</span>
          <span><T id="waybill.gl.finalApproveGl" locale={locale} /></span>
        </button>
      </form>,
    );
    headerActions.push(
      <Link
        key="reject"
        href={`/waybill/${waybillId}?action=final-reject&stage=final_authorization`}
        className="inline-flex items-center gap-1.5 rounded-lg bg-critical px-3 py-1.5 text-sm font-bold text-paper hover:bg-critical-strong"
      >
        <span aria-hidden>✗</span>
        <span><T id="waybill.gl.reject" locale={locale} /></span>
      </Link>,
    );
  }
  if (canConfirmGl && isDisbursed) {
    headerActions.push(
      <form key="confirm" action={confirmGlRecordedAction}>
        <input type="hidden" name="waybillId" value={waybillId} />
        <input
          type="hidden"
          name="expenseId"
          value={String(journal.draft?.journal_id ?? journal.posted?.journal_id ?? 0)}
        />
        <button
          type="submit"
          className="inline-flex items-center gap-1.5 rounded-lg bg-caution px-3 py-1.5 text-sm font-bold text-ink hover:bg-caution-strong"
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
      <p key="posted-by" className="rounded-md border border-positive bg-positive-soft p-2 font-mono text-positive-soft">
        <span className="text-positive">posted_by:</span>{' '}
        {posted.finalized_by_name ?? '—'}{' '}
        <span className="text-mute">#{posted.finalized_by ?? '—'}</span>
        {posted.finalized_at && (
          <>
            <span className="mx-1 text-mute">·</span>
            <span className="text-positive">{fmtTs(posted.finalized_at, locale === 'de' ? 'en' as const : locale)}</span>
          </>
        )}
      </p>,
    );
  }
  if (posted_event) {
    meta.push(
      <p key="posted-event" className="rounded-md border border-positive bg-positive-soft p-2 font-mono text-positive-soft">
        <span className="text-positive">posted_event:</span>{' '}
        {posted_event.actor_name ?? '—'}{' '}
        <span className="text-mute">#{posted_event.actor_id ?? '—'}</span>
        <span className="mx-1 text-mute">·</span>
        <span className="text-positive">{fmtTs(posted_event.occurred_at, locale === 'de' ? 'en' as const : locale)}</span>
      </p>,
    );
  }
  if (confirmed_event) {
    meta.push(
      <p key="confirmed-event" className="rounded-md border border-info bg-info-soft p-2 font-mono text-info-soft sm:col-span-2">
        <span className="text-info">confirmed_event:</span>{' '}
        {confirmed_event.actor_name ?? '—'}{' '}
        <span className="text-mute">#{confirmed_event.actor_id ?? '—'}</span>
        <span className="mx-1 text-mute">·</span>
        <span className="text-info">{fmtTs(confirmed_event.occurred_at, locale === 'de' ? 'en' as const : locale)}</span>
      </p>,
    );
  }

  return (
    <section className="space-y-4">
      {headerActions.length > 0 && (
        <div className="flex flex-wrap items-center gap-2">{headerActions}</div>
      )}

      {empty && (
        <p className="bg-paper-2 border border-rule rounded-md border p-3 text-sm italic text-mute">
          <T id="waybill.gl.noJournalYet" locale={locale} />
        </p>
      )}

      {draft && (
        <section className="space-y-2 rounded-md border border-info bg-info-soft p-3">
          <header className="flex flex-wrap items-baseline justify-between gap-2">
            <div className="flex items-center gap-2">
              <span className="rounded-md border border-info bg-info px-2 py-0.5 text-sm font-mono font-bold uppercase text-info-soft">
                📝 DRAFT
              </span>
              <span className="font-mono text-sm text-mute">JE #{draft.journal_id}</span>
            </div>
            <span className="font-mono text-sm text-mute">
              entry_date: <span className="text-info">{fmtDate(draft.entry_date)}</span>
            </span>
          </header>
          {draft.description && <p className="text-sm text-ink-2">{draft.description}</p>}
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
        <section className="space-y-2 rounded-md border border-positive bg-positive-soft p-3">
          <header className="flex flex-wrap items-baseline justify-between gap-2">
            <div className="flex items-center gap-2">
              <span className="rounded-md border border-positive bg-positive px-2 py-0.5 text-sm font-mono font-bold uppercase text-positive-soft">
                ✓ POSTED
              </span>
              <span className="font-mono text-sm text-mute">JE #{posted.journal_id}</span>
            </div>
            <span className="font-mono text-sm text-mute">
              entry_date: <span className="text-positive">{fmtDate(posted.entry_date)}</span>
            </span>
          </header>
          {posted.description && <p className="text-sm text-ink-2">{posted.description}</p>}
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
    </section>
  );
}

export function ProcurementGlPostConfirm({
  waybillId,
  step,
  stepNo,
  canSave,
  canPost,
  canConfirm,
  tone,
  actorCanSeeLines,
  locale,
}: {
  waybillId: string;
  step: ProcurementJournalStepView;
  stepNo: 1 | 2;
  canSave: boolean;
  canPost: boolean;
  canConfirm: boolean;
  tone: 'cyan' | 'amber';
  actorCanSeeLines: boolean;
  locale: Locale;
}) {
  const { draft, posted, posted_event, confirmed_event } = step;
  const liveLines = draft?.lines ?? posted?.lines ?? [];
  const totals = summarizeLines(liveLines);

  const actions: React.ReactNode[] = [];
  if (canSave && draft && !posted) {
    actions.push(
      <form key="save" action={saveProcurementAccrualAction}>
        <input type="hidden" name="waybillId" value={waybillId} />
        <button
          type="submit"
          className="rounded-lg border border-info bg-info px-3 py-1.5 text-sm font-mono text-info-soft hover:bg-info"
        >
          ⟳ <T id="waybill.gl.resaveDraft" locale={locale} />
        </button>
      </form>,
    );
  }
  if (canPost && draft && !posted) {
    const action = stepNo === 1 ? postProcurementAccrualAction : postProcurementSettlementAction;
    actions.push(
      <form key="post" action={action}>
        <input type="hidden" name="waybillId" value={waybillId} />
        <input type="hidden" name="journalId" value={String(draft.journal_id)} />
        <button
          type="submit"
          className={
            'inline-flex items-center gap-1.5 rounded-lg px-3 py-1.5 text-sm font-bold text-ink shadow ' +
            (tone === 'amber'
              ? 'bg-caution text-ink hover:bg-caution-strong'
              : 'bg-positive text-paper hover:bg-positive-strong')
          }
        >
          <span aria-hidden>✓</span>
          <span>
            <T id={stepNo === 1 ? 'waybill.gl.commitPostingAccrual' : 'waybill.gl.commitPostingSettlement'} locale={locale} />
          </span>
        </button>
      </form>,
    );
  }
  if (canConfirm && posted && !confirmed_event) {
    actions.push(
      <form key="confirm" action={confirmProcurementGlAction}>
        <input type="hidden" name="waybillId" value={waybillId} />
        <input type="hidden" name="step" value={stepNo === 1 ? 'accrual' : 'settlement'} />
        <button
          type="submit"
          className="inline-flex items-center gap-1.5 rounded-lg bg-caution px-3 py-1.5 text-sm font-bold text-ink hover:bg-caution-strong"
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
      <p key="posted-by" className="rounded-md border border-positive bg-positive-soft p-2 font-mono text-positive-soft">
        <span className="text-positive">posted_by:</span>{' '}
        {posted.finalized_by_name ?? '—'}{' '}
        <span className="text-mute">#{posted.finalized_by ?? '—'}</span>
        {posted.finalized_at && (
          <>
            <span className="mx-1 text-mute">·</span>
            <span className="text-positive">{fmtTs(posted.finalized_at, locale === 'de' ? 'en' as const : locale)}</span>
          </>
        )}
      </p>,
    );
  }
  if (posted_event) {
    meta.push(
      <p key="posted-event" className="rounded-md border border-positive bg-positive-soft p-2 font-mono text-positive-soft">
        <span className="text-positive">posted_event:</span>{' '}
        {posted_event.actor_name ?? '—'}{' '}
        <span className="text-mute">#{posted_event.actor_id ?? '—'}</span>
        <span className="mx-1 text-mute">·</span>
        <span className="text-positive">{fmtTs(posted_event.occurred_at, locale === 'de' ? 'en' as const : locale)}</span>
      </p>,
    );
  }
  if (confirmed_event) {
    meta.push(
      <p key="confirmed-event" className="rounded-md border border-info bg-info-soft p-2 font-mono text-info-soft sm:col-span-2">
        <span className="text-info">confirmed_event:</span>{' '}
        {confirmed_event.actor_name ?? '—'}{' '}
        <span className="text-mute">#{confirmed_event.actor_id ?? '—'}</span>
        <span className="mx-1 text-mute">·</span>
        <span className="text-info">{fmtTs(confirmed_event.occurred_at, locale === 'de' ? 'en' as const : locale)}</span>
      </p>,
    );
  }

  return (
    <section className="space-y-3">
      {actions.length > 0 && (
        <div className="flex flex-wrap items-center gap-2">{actions}</div>
      )}

      {draft && (
        <section className="space-y-2 rounded-lg border border-info bg-info-soft p-3">
          <header className="flex flex-wrap items-baseline justify-between gap-2">
            <div className="flex items-center gap-2">
              <span className="rounded-md border border-info bg-info px-2 py-0.5 text-sm font-mono font-bold uppercase text-info-soft">
                📝 DRAFT
              </span>
              <span className="font-mono text-sm text-mute">JE #{draft.journal_id}</span>
            </div>
            <span className="font-mono text-sm text-mute">
              entry_date: <span className="text-info">{fmtDate(draft.entry_date)}</span>
            </span>
          </header>
          {draft.description && <p className="text-sm text-ink-2">{draft.description}</p>}
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
        <section className="space-y-2 rounded-lg border border-positive bg-positive-soft p-3">
          <header className="flex flex-wrap items-baseline justify-between gap-2">
            <div className="flex items-center gap-2">
              <span className="rounded-md border border-positive bg-positive px-2 py-0.5 text-sm font-mono font-bold uppercase text-positive-soft">
                ✓ POSTED
              </span>
              <span className="font-mono text-sm text-mute">JE #{posted.journal_id}</span>
            </div>
            <span className="font-mono text-sm text-mute">
              entry_date: <span className="text-positive">{fmtDate(posted.entry_date)}</span>
            </span>
          </header>
          {posted.description && <p className="text-sm text-ink-2">{posted.description}</p>}
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
    </section>
  );
}
