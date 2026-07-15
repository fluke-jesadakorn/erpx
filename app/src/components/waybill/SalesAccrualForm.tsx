import React from 'react';
import { fmtDate, fmtTs } from './ui';
import { GlVisibilityGate } from './GlVisibilityGate';
import { Bilingual } from '@/components/i18n/Bilingual';
import {
  confirmSalesGlAction,
  postSalesGlAccrualAction,
  postSalesGlSettlementAction,
  postSalesGlVatAction,
} from '@/app/actions';
import type { ProcurementJournalStepView, SalesJournalView } from '@folio-lib/waybill/queries';
import { LinesTable, summarizeLines } from './GlLinesTable';

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
        title={<Bilingual en="VAT" th="ภาษีมูลค่าเพิ่ม" locale={locale} />}
        subtitle={
          <Bilingual en="Dr output VAT receivable · Cr output VAT payable" th="Dr ภาษีซื้อรอเรียกเก็บ · Cr ภาษีขาย" locale={locale} />
        }
        tone="cyan"
        actorCanSeeLines={actorCanSeeLines}
        canPost={canPostVat}
        canConfirm={canConfirm}
        postAction={postSalesGlVatAction}
        postLabel={<Bilingual en="Post VAT" th="บันทึกบัญชี (USt)" locale={locale} />}
        locale={locale}
      />
      <SalesStep
        waybillId={waybillId}
        step={journal.accrual}
        stepNo={2}
        emoji="📒"
        title={<Bilingual en="Accrual (Revenue)" th="ตั้งหนี้ (รายได้)" locale={locale} />}
        subtitle={
          <Bilingual en="Dr AR · Cr sales revenue" th="Dr ลูกหนี้การค้า · Cr รายได้จากการขาย" locale={locale} />
        }
        tone="emerald"
        actorCanSeeLines={actorCanSeeLines}
        canPost={canPostAccrual}
        canConfirm={canConfirm}
        postAction={postSalesGlAccrualAction}
        postLabel={<Bilingual en="Post accrual" th="บันทึกบัญชี (ตั้งหนี้)" locale={locale} />}
        locked={!stepVatPosted}
        blockedReason={
          !stepVatPosted ? (
            <Bilingual en="Locked until Step 1 (VAT) is posted" th="รอให้บันทึกขั้นที่ 1 (USt) ให้เสร็จก่อน" locale={locale} />
          ) : null
        }
        locale={locale}
      />
      <SalesStep
        waybillId={waybillId}
        step={journal.settlement}
        stepNo={3}
        emoji="💳"
        title={<Bilingual en="Settlement (collection)" th="ชำระ (รับเงิน)" locale={locale} />}
        subtitle={
          <Bilingual en="Dr cash at bank · Cr AR" th="Dr เงินฝากธนาคาร · Cr ลูกหนี้การค้า" locale={locale} />
        }
        tone="amber"
        actorCanSeeLines={actorCanSeeLines}
        canPost={canPostSettlement}
        canConfirm={canConfirm}
        postAction={postSalesGlSettlementAction}
        postLabel={<Bilingual en="Post settlement" th="บันทึกบัญชี (ชำระ)" locale={locale} />}
        locked={!stepAccrualPosted}
        blockedReason={
          !stepAccrualPosted ? (
            <Bilingual en="Locked until Step 2 (Accrual) is posted" th="รอให้บันทึกขั้นที่ 2 (ตั้งหนี้) ให้เสร็จก่อน" locale={locale} />
          ) : null
        }
        locale={locale}
      />
      <p className="font-mono text-sm uppercase tracking-widest text-slate-500">
        <Bilingual en="Each step locked until the previous is posted" th="ขั้นถัดไปจะล็อคจนกว่าขั้นก่อนหน้าจะถูกบันทึก" locale={locale} />
        {!stepSettlementPosted && (
          <>
            {' · '}
            <span className="text-slate-600">
              <Bilingual en="settlement posts at so_paid" th="ชำระจะโพสต์ที่ so_paid" locale={locale} />
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
            'inline-flex items-center gap-1.5 rounded-lg px-3 py-1.5 text-sm font-bold text-slate-950 shadow ' +
            (tone === 'amber'
              ? 'bg-caution text-ink hover:bg-caution-strong'
              : 'bg-positive text-paper hover:bg-positive-strong')
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
    <section className={'space-y-3 rounded-xl border ' + (tone === 'cyan'
      ? 'border-cyan-500/40 bg-cyan-950/15'
      : tone === 'amber'
        ? 'border-amber-500/40 bg-amber-950/15'
        : 'border-emerald-500/40 bg-emerald-950/15')}>
      <header className="flex flex-wrap items-baseline justify-between gap-3 px-4 py-3">
        <div className="flex items-center gap-3">
          <span aria-hidden className="grid h-9 w-9 place-items-center rounded-xl bg-info-soft text-info text-lg ring-1 ring-info/40">
            {emoji}
          </span>
          <div className="flex flex-col">
            <div className="flex items-center gap-2 text-base font-bold text-white">
              <span className="glass-panel rounded-md border px-1.5 py-0.5 font-mono text-sm uppercase tracking-widest text-slate-300">
                {<Bilingual en={`Step ${stepNo}`} th={`ขั้นที่ ${stepNo}`} locale={locale} />}
              </span>
              <span>{title}</span>
            </div>
            <span className="font-mono text-sm uppercase tracking-widest text-slate-500">{subtitle}</span>
          </div>
        </div>
        {actions.length > 0 && (
          <div className="flex flex-wrap items-center gap-2">{actions}</div>
        )}
      </header>

      <div className={'space-y-3 border-t border-slate-800/60 px-4 py-3 ' + (locked ? 'pointer-events-none opacity-60' : '')}>
        {blockedReason && (
          <p className="glass-panel rounded-md border p-3 text-sm italic text-slate-500">{blockedReason}</p>
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
      </div>
    </section>
  );
}