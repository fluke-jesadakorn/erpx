import React from 'react';
import Link from 'next/link';
import { fmtDate, fmtTs } from './ui';
import { GlVisibilityGate } from './GlVisibilityGate';
import { Bilingual } from '@/components/i18n/Bilingual';
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
import type { ExpenseJournalView, ProcurementJournalStepView } from '@folio-lib/waybill/queries';
import { LinesTable, summarizeLines } from './GlLinesTable';

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
          className="rounded-lg border border-cyan-500/40 bg-cyan-500/10 px-3 py-1.5 text-sm font-mono text-cyan-200 hover:bg-cyan-500/30"
        >
          ⟳ {<Bilingual en="Recompute draft" th="คำนวณร่างใหม่" de="Entwurf neu berechnen" locale={locale} />}
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
          <span>{<Bilingual en="Final approve → GL" th="อนุมัติขั้นสุดท้าย → GL" de="Endgültig freigeben → GL" locale={locale} />}</span>
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
        <span>{<Bilingual en="Final reject" th="ปฏิเสธ" de="Endgültig ablehnen" locale={locale} />}</span>
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
          <span>{<Bilingual en="Confirm GL" th="ยืนยัน GL" de="GL bestätigen" locale={locale} />}</span>
        </button>
      </form>,
    );
  }

  const meta: React.ReactNode[] = [];
  if (posted) {
    meta.push(
      <p key="posted-by" className="rounded-md border border-emerald-500/40 bg-emerald-950/40 p-2 font-mono text-emerald-100">
        <span className="text-emerald-300">posted_by:</span>{' '}
        {posted.finalized_by_name ?? '—'}{' '}
        <span className="text-slate-500">#{posted.finalized_by ?? '—'}</span>
        {posted.finalized_at && (
          <>
            <span className="mx-1 text-slate-700">·</span>
            <span className="text-emerald-300/80">{fmtTs(posted.finalized_at, locale === 'de' ? 'en' as const : locale)}</span>
          </>
        )}
      </p>,
    );
  }
  if (posted_event) {
    meta.push(
      <p key="posted-event" className="rounded-md border border-emerald-500/40 bg-emerald-950/40 p-2 font-mono text-emerald-100">
        <span className="text-emerald-300">posted_event:</span>{' '}
        {posted_event.actor_name ?? '—'}{' '}
        <span className="text-slate-500">#{posted_event.actor_id ?? '—'}</span>
        <span className="mx-1 text-slate-700">·</span>
        <span className="text-emerald-300/80">{fmtTs(posted_event.occurred_at, locale === 'de' ? 'en' as const : locale)}</span>
      </p>,
    );
  }
  if (confirmed_event) {
    meta.push(
      <p key="confirmed-event" className="rounded-md border border-cyan-500/40 bg-cyan-950/40 p-2 font-mono text-cyan-100 sm:col-span-2">
        <span className="text-cyan-300">confirmed_event:</span>{' '}
        {confirmed_event.actor_name ?? '—'}{' '}
        <span className="text-slate-500">#{confirmed_event.actor_id ?? '—'}</span>
        <span className="mx-1 text-slate-700">·</span>
        <span className="text-cyan-300/80">{fmtTs(confirmed_event.occurred_at, locale === 'de' ? 'en' as const : locale)}</span>
      </p>,
    );
  }

  return (
    <section className="space-y-4">
      {headerActions.length > 0 && (
        <div className="flex flex-wrap items-center gap-2">{headerActions}</div>
      )}

      {empty && (
        <p className="glass-panel rounded-md border p-3 text-sm italic text-slate-500">
          <Bilingual en="No GL journal yet — will be created on submission" th="ยังไม่มีสมุดบัญชี — จะถูกสร้างเมื่อส่งเบิก" locale={locale} />
        </p>
      )}

      {draft && (
        <section className="space-y-2 rounded-xl border border-cyan-500/40 bg-cyan-950/15 p-3">
          <header className="flex flex-wrap items-baseline justify-between gap-2">
            <div className="flex items-center gap-2">
              <span className="rounded-md border border-cyan-400/40 bg-cyan-500/15 px-2 py-0.5 text-sm font-mono font-bold uppercase text-cyan-200">
                📝 DRAFT
              </span>
              <span className="font-mono text-sm text-slate-500">JE #{draft.journal_id}</span>
            </div>
            <span className="font-mono text-sm text-slate-500">
              entry_date: <span className="text-cyan-300">{fmtDate(draft.entry_date)}</span>
            </span>
          </header>
          {draft.description && <p className="text-sm text-slate-300">{draft.description}</p>}
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
        <section className="space-y-2 rounded-xl border border-emerald-500/40 bg-emerald-950/15 p-3">
          <header className="flex flex-wrap items-baseline justify-between gap-2">
            <div className="flex items-center gap-2">
              <span className="rounded-md border border-emerald-400/40 bg-emerald-500/15 px-2 py-0.5 text-sm font-mono font-bold uppercase text-emerald-200">
                ✓ POSTED
              </span>
              <span className="font-mono text-sm text-slate-500">JE #{posted.journal_id}</span>
            </div>
            <span className="font-mono text-sm text-slate-500">
              entry_date: <span className="text-emerald-300">{fmtDate(posted.entry_date)}</span>
            </span>
          </header>
          {posted.description && <p className="text-sm text-slate-300">{posted.description}</p>}
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
          className="rounded-lg border border-cyan-500/40 bg-cyan-500/10 px-3 py-1.5 text-sm font-mono text-cyan-200 hover:bg-cyan-500/30"
        >
          ⟳ {<Bilingual en="Re-save draft" th="บันทึกร่างใหม่" de="Entwurf neu speichern" locale={locale} />}
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
            'inline-flex items-center gap-1.5 rounded-lg px-3 py-1.5 text-sm font-bold text-slate-950 shadow ' +
            (tone === 'amber'
              ? 'bg-caution text-ink hover:bg-caution-strong'
              : 'bg-positive text-paper hover:bg-positive-strong')
          }
        >
          <span aria-hidden>✓</span>
          <span>
            {stepNo === 1
              ? <Bilingual en="Post accrual" th="บันทึกบัญชี (ก่อนจ่าย)" de="Buchen (vor Zahlung)" locale={locale} />
              : <Bilingual en="Post settlement" th="บันทึกบัญชี (หลังจ่าย)" de="Buchen (nach Zahlung)" locale={locale} />}
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
          <span>{<Bilingual en="Confirm GL" th="ยืนยัน GL" de="GL bestätigen" locale={locale} />}</span>
        </button>
      </form>,
    );
  }

  const meta: React.ReactNode[] = [];
  if (posted) {
    meta.push(
      <p key="posted-by" className="rounded-md border border-emerald-500/40 bg-emerald-950/40 p-2 font-mono text-emerald-100">
        <span className="text-emerald-300">posted_by:</span>{' '}
        {posted.finalized_by_name ?? '—'}{' '}
        <span className="text-slate-500">#{posted.finalized_by ?? '—'}</span>
        {posted.finalized_at && (
          <>
            <span className="mx-1 text-slate-700">·</span>
            <span className="text-emerald-300/80">{fmtTs(posted.finalized_at, locale === 'de' ? 'en' as const : locale)}</span>
          </>
        )}
      </p>,
    );
  }
  if (posted_event) {
    meta.push(
      <p key="posted-event" className="rounded-md border border-emerald-500/40 bg-emerald-950/40 p-2 font-mono text-emerald-100">
        <span className="text-emerald-300">posted_event:</span>{' '}
        {posted_event.actor_name ?? '—'}{' '}
        <span className="text-slate-500">#{posted_event.actor_id ?? '—'}</span>
        <span className="mx-1 text-slate-700">·</span>
        <span className="text-emerald-300/80">{fmtTs(posted_event.occurred_at, locale === 'de' ? 'en' as const : locale)}</span>
      </p>,
    );
  }
  if (confirmed_event) {
    meta.push(
      <p key="confirmed-event" className="rounded-md border border-cyan-500/40 bg-cyan-950/40 p-2 font-mono text-cyan-100 sm:col-span-2">
        <span className="text-cyan-300">confirmed_event:</span>{' '}
        {confirmed_event.actor_name ?? '—'}{' '}
        <span className="text-slate-500">#{confirmed_event.actor_id ?? '—'}</span>
        <span className="mx-1 text-slate-700">·</span>
        <span className="text-cyan-300/80">{fmtTs(confirmed_event.occurred_at, locale === 'de' ? 'en' as const : locale)}</span>
      </p>,
    );
  }

  return (
    <section className="space-y-3">
      {actions.length > 0 && (
        <div className="flex flex-wrap items-center gap-2">{actions}</div>
      )}

      {draft && (
        <section className="space-y-2 rounded-lg border border-cyan-500/40 bg-cyan-950/15 p-3">
          <header className="flex flex-wrap items-baseline justify-between gap-2">
            <div className="flex items-center gap-2">
              <span className="rounded-md border border-cyan-400/40 bg-cyan-500/15 px-2 py-0.5 text-sm font-mono font-bold uppercase text-cyan-200">
                📝 DRAFT
              </span>
              <span className="font-mono text-sm text-slate-500">JE #{draft.journal_id}</span>
            </div>
            <span className="font-mono text-sm text-slate-500">
              entry_date: <span className="text-cyan-300">{fmtDate(draft.entry_date)}</span>
            </span>
          </header>
          {draft.description && <p className="text-sm text-slate-300">{draft.description}</p>}
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
        <section className="space-y-2 rounded-lg border border-emerald-500/40 bg-emerald-950/15 p-3">
          <header className="flex flex-wrap items-baseline justify-between gap-2">
            <div className="flex items-center gap-2">
              <span className="rounded-md border border-emerald-400/40 bg-emerald-500/15 px-2 py-0.5 text-sm font-mono font-bold uppercase text-emerald-200">
                ✓ POSTED
              </span>
              <span className="font-mono text-sm text-slate-500">JE #{posted.journal_id}</span>
            </div>
            <span className="font-mono text-sm text-slate-500">
              entry_date: <span className="text-emerald-300">{fmtDate(posted.entry_date)}</span>
            </span>
          </header>
          {posted.description && <p className="text-sm text-slate-300">{posted.description}</p>}
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