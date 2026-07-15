import React from 'react';
import type {
  ExpenseJournalView,
  ProcurementJournalView,
  SalesJournalView,
  WaybillJournalView,
} from '@folio-lib/waybill/queries';
import { Bilingual } from '@/components/i18n/Bilingual';
import { ExpenseGlPostConfirm, ProcurementGlPostConfirm } from './GlPostConfirm';
import { SalesAccrualForm } from './SalesAccrualForm';

type Locale = 'th' | 'de';

type Props = {
  waybillId: string;
  origin: 'expense' | 'pr' | 'po' | 'so';
  journal: WaybillJournalView;
  amountTHB?: number | null;
  actorRole: string | null;
  actorCanSeeLines: boolean;
  lang?: Locale;
  canFinalApprove?: boolean;
  canConfirmGl?: boolean;
  isFinalApproval?: boolean;
  isDisbursed?: boolean;
  hasPostedToGl?: boolean;
  hasGlConfirmed?: boolean;
  canSaveAccrual?: boolean;
  canPostAccrual?: boolean;
  canConfirmAccrual?: boolean;
  canPostSettlement?: boolean;
  canConfirmSettlement?: boolean;
  isAccountingAuthorization?: boolean;
  canPostSalesGlVat?: boolean;
  canPostSalesGlAccrual?: boolean;
  canPostSalesGlSettlement?: boolean;
  canConfirmSalesGl?: boolean;
};

function DraftChip() {
  return (
    <span className="inline-flex items-center gap-1.5 rounded-full border border-cyan-500/40 bg-cyan-500/15 px-2.5 py-1 text-sm font-mono font-bold uppercase text-cyan-200">
      <span aria-hidden>📝</span>
      <span>DRAFT</span>
    </span>
  );
}

function ExpenseBodyInlineBadges({ j }: { j: ExpenseJournalView }) {
  return <>{j.draft && <DraftChip />}</>;
}

function ProcurementBodyInlineBadges({ j, locale }: { j: ProcurementJournalView; locale: Locale }) {
  return (
    <>
      {j.accrual.draft && <DraftChip />}
      {j.settlement.draft && (
        <span className="inline-flex items-center gap-1.5 rounded-full border border-amber-500/40 bg-amber-500/15 px-2.5 py-1 text-sm font-mono font-bold uppercase text-amber-200">
          <span aria-hidden>💳</span>
          <span>
            <Bilingual en="SETTLE DRAFT" th="ร่างหลังจ่าย" locale={locale} />
          </span>
        </span>
      )}
      {j.settlement.posted && (
        <span className="inline-flex items-center gap-1.5 rounded-full border border-amber-500/40 bg-amber-500/15 px-2.5 py-1 text-sm font-mono font-bold uppercase text-amber-200">
          <span aria-hidden>💳</span>
          <span>
            <Bilingual en="SETTLE POSTED" th="บันทึกหลังจ่าย" locale={locale} />
          </span>
        </span>
      )}
    </>
  );
}

function SalesBodyInlineBadges({ j, locale }: { j: SalesJournalView; locale: Locale }) {
  return (
    <>
      {j.vat.draft && <DraftChip />}
      {j.accrual.draft && (
        <span className="inline-flex items-center gap-1.5 rounded-full border border-emerald-500/40 bg-emerald-500/15 px-2.5 py-1 text-sm font-mono font-bold uppercase text-emerald-200">
          <span aria-hidden>📒</span>
          <span>
            <Bilingual en="ACCRUAL DRAFT" th="ร่างตั้งหนี้" locale={locale} />
          </span>
        </span>
      )}
      {j.accrual.posted && (
        <span className="inline-flex items-center gap-1.5 rounded-full border border-emerald-500/40 bg-emerald-500/15 px-2.5 py-1 text-sm font-mono font-bold uppercase text-emerald-200">
          <span aria-hidden>📒</span>
          <span>
            <Bilingual en="ACCRUAL POSTED" th="บันทึกตั้งหนี้" locale={locale} />
          </span>
        </span>
      )}
      {j.settlement.draft && (
        <span className="inline-flex items-center gap-1.5 rounded-full border border-amber-500/40 bg-amber-500/15 px-2.5 py-1 text-sm font-mono font-bold uppercase text-amber-200">
          <span aria-hidden>💳</span>
          <span>
            <Bilingual en="SETTLE DRAFT" th="ร่างชำระ" locale={locale} />
          </span>
        </span>
      )}
      {j.settlement.posted && (
        <span className="inline-flex items-center gap-1.5 rounded-full border border-amber-500/40 bg-amber-500/15 px-2.5 py-1 text-sm font-mono font-bold uppercase text-amber-200">
          <span aria-hidden>💳</span>
          <span>
            <Bilingual en="SETTLE POSTED" th="บันทึกชำระ" locale={locale} />
          </span>
        </span>
      )}
    </>
  );
}

export function WaybillGlSection(props: Props) {
  const locale = (props.lang ?? 'th') as Locale;
  const isExpense = props.journal.kind === 'expense';
  const isSales = props.journal.kind === 'sales';
  const isProcurement = !isExpense && !isSales;

  const title = isExpense
    ? <Bilingual en="GL Journal" th="สมุดบัญชี (GL)" locale={locale} />
    : isSales
      ? <Bilingual en="GL Journal — 3 steps" th="สมุดบัญชี (GL) — 3 ขั้น" locale={locale} />
      : <Bilingual en="GL Journal — 2 steps" th="สมุดบัญชี (GL) — 2 ขั้น" locale={locale} />;

  const subtitle = isExpense
    ? <Bilingual en="draft + posted · debit/credit balance check" th="ร่าง + บันทึกแล้ว · ตรวจสอบเดบิต/เครดิต" locale={locale} />
    : isSales
      ? <Bilingual en="VAT · accrual · settlement · debit/credit" th="USt · ตั้งหนี้ · ชำระ · เดบิต/เครดิต" locale={locale} />
      : <Bilingual en="before pay · after pay · debit/credit" th="ก่อนจ่าย · หลังจ่าย · เดบิต/เครดิต" locale={locale} />;

  return (
    <details className="glass-panel group rounded-2xl border" open>
      <summary className="cursor-pointer list-none px-4 py-3 [&::-webkit-details-marker]:hidden">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div className="flex items-center gap-3">
            <span
              aria-hidden
              className="grid h-9 w-9 place-items-center rounded-xl bg-info-soft text-info text-lg ring-1 ring-info/40"
            >
              📒
            </span>
            <div className="flex flex-col">
              <span className="text-base font-bold text-white">{title}</span>
              <span className="font-mono text-sm uppercase tracking-widest text-slate-500">
                {subtitle} · <span className="text-cyan-300">{props.waybillId}</span>
              </span>
            </div>
          </div>
          <div className="flex items-center gap-2">
            {isExpense ? (
              <ExpenseBodyInlineBadges j={props.journal as unknown as ExpenseJournalView} />
            ) : isSales ? (
              <SalesBodyInlineBadges j={props.journal as unknown as SalesJournalView} locale={locale} />
            ) : (
              <ProcurementBodyInlineBadges
                j={props.journal as unknown as ProcurementJournalView}
                locale={locale}
              />
            )}
            <span className="font-mono text-sm uppercase tracking-wider text-slate-500 group-open:hidden">▶</span>
            <span className="font-mono text-sm uppercase tracking-wider text-slate-500 hidden group-open:inline">▼</span>
          </div>
        </div>
      </summary>

      {isExpense ? (
        <ExpenseGlPostConfirm
          waybillId={props.waybillId}
          journal={props.journal as unknown as ExpenseJournalView}
          canFinalApprove={props.canFinalApprove ?? false}
          canConfirmGl={props.canConfirmGl ?? false}
          isFinalApproval={props.isFinalApproval ?? false}
          isDisbursed={props.isDisbursed ?? false}
          actorCanSeeLines={props.actorCanSeeLines}
          locale={locale}
        />
      ) : isSales ? (
        <SalesAccrualForm
          waybillId={props.waybillId}
          journal={props.journal as unknown as SalesJournalView}
          actorCanSeeLines={props.actorCanSeeLines}
          canPostVat={props.canPostSalesGlVat ?? false}
          canPostAccrual={props.canPostSalesGlAccrual ?? false}
          canPostSettlement={props.canPostSalesGlSettlement ?? false}
          canConfirm={props.canConfirmSalesGl ?? false}
          locale={locale}
        />
      ) : (
        <ProcurementGlPostConfirm
          waybillId={props.waybillId}
          step={(props.journal as unknown as ProcurementJournalView).accrual}
          stepNo={1}
          canSave={props.canSaveAccrual ?? false}
          canPost={props.canPostAccrual ?? false}
          canConfirm={props.canConfirmAccrual ?? false}
          tone="cyan"
          actorCanSeeLines={props.actorCanSeeLines}
          locale={locale}
        />
      )}

      {isProcurement && (
        <ProcurementGlPostConfirm
          waybillId={props.waybillId}
          step={(props.journal as unknown as ProcurementJournalView).settlement}
          stepNo={2}
          canSave={false}
          canPost={props.canPostSettlement ?? false}
          canConfirm={props.canConfirmSettlement ?? false}
          tone="amber"
          actorCanSeeLines={props.actorCanSeeLines}
          locale={locale}
        />
      )}
    </details>
  );
}