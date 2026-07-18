import React from 'react';
import type {
  ExpenseJournalView,
  ProcurementJournalView,
  SalesJournalView,
  WaybillJournalView,
} from '@/waybill/queries';
import { ExpenseGlPostConfirm, ProcurementGlPostConfirm } from './GlPostConfirm';
import { SalesAccrualForm } from './SalesAccrualForm';
import { T } from '@/components/i18n/TServer';

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
  canEditDraft?: boolean;
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
    <span className="inline-flex items-center gap-1.5 rounded-full border border-info bg-info px-2.5 py-1 text-sm font-mono font-bold uppercase text-info-soft">
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
        <span className="inline-flex items-center gap-1.5 rounded-full border border-caution bg-caution px-2.5 py-1 text-sm font-mono font-bold uppercase text-caution-soft">
          <span aria-hidden>💳</span>
          <span>
            <T id="waybill.gl.settleDraftBadge" locale={locale} />
          </span>
        </span>
      )}
      {j.settlement.posted && (
        <span className="inline-flex items-center gap-1.5 rounded-full border border-caution bg-caution px-2.5 py-1 text-sm font-mono font-bold uppercase text-caution-soft">
          <span aria-hidden>💳</span>
          <span>
            <T id="waybill.gl.settlePostedBadge" locale={locale} />
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
        <span className="inline-flex items-center gap-1.5 rounded-full border border-positive bg-positive px-2.5 py-1 text-sm font-mono font-bold uppercase text-positive-soft">
          <span aria-hidden>📒</span>
          <span>
            <T id="waybill.gl.accrualDraftBadge" locale={locale} />
          </span>
        </span>
      )}
      {j.accrual.posted && (
        <span className="inline-flex items-center gap-1.5 rounded-full border border-positive bg-positive px-2.5 py-1 text-sm font-mono font-bold uppercase text-positive-soft">
          <span aria-hidden>📒</span>
          <span>
            <T id="waybill.gl.accrualPostedBadge" locale={locale} />
          </span>
        </span>
      )}
      {j.settlement.draft && (
        <span className="inline-flex items-center gap-1.5 rounded-full border border-caution bg-caution px-2.5 py-1 text-sm font-mono font-bold uppercase text-caution-soft">
          <span aria-hidden>💳</span>
          <span>
            <T id="waybill.gl.settleDraftBadge" locale={locale} />
          </span>
        </span>
      )}
      {j.settlement.posted && (
        <span className="inline-flex items-center gap-1.5 rounded-full border border-caution bg-caution px-2.5 py-1 text-sm font-mono font-bold uppercase text-caution-soft">
          <span aria-hidden>💳</span>
          <span>
            <T id="waybill.gl.settlePostedBadge" locale={locale} />
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
    ? <T id="waybill.gl.journal" locale={locale} />
    : isSales
      ? <T id="waybill.gl.journal3" locale={locale} />
      : <T id="waybill.gl.journal2" locale={locale} />;

  const subtitle = isExpense
    ? <T id="waybill.gl.subE1" locale={locale} />
    : isSales
      ? <T id="waybill.gl.subE2" locale={locale} />
      : <T id="waybill.gl.subE3" locale={locale} />;

  return (
    <details className="bg-paper-2 border border-rule group rounded-md border" open>
      <summary className="cursor-pointer list-none px-4 py-3 [&::-webkit-details-marker]:hidden">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div className="flex items-center gap-3">
            <span
              aria-hidden
              className="grid h-9 w-9 place-items-center rounded-md bg-info-soft text-info text-lg ring-1 ring-info/40"
            >
              📒
            </span>
            <div className="flex flex-col">
              <span className="text-base font-bold text-ink">{title}</span>
              <span className="font-mono text-sm uppercase tracking-widest text-mute">
                {subtitle} · <span className="text-info">{props.waybillId}</span>
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
            <span className="font-mono text-sm uppercase tracking-wider text-mute group-open:hidden">▶</span>
            <span className="font-mono text-sm uppercase tracking-wider text-mute hidden group-open:inline">▼</span>
          </div>
        </div>
      </summary>

      {isExpense ? (
        <ExpenseGlPostConfirm
          waybillId={props.waybillId}
          journal={props.journal as unknown as ExpenseJournalView}
          canFinalApprove={props.canFinalApprove ?? false}
          canConfirmGl={props.canConfirmGl ?? false}
          canEditDraft={props.canEditDraft ?? false}
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
