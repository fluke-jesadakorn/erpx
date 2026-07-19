'use client';

import { Activity, useState } from 'react';
import {
  ArrowUpRight,
  Building2,
  ChevronDown,
  CircleAlert,
  CircleCheck,
  Landmark,
  Loader2,
  Lock,
  Receipt,
  Upload,
  UserRound,
} from 'lucide-react';
import {
  SlipUpload,
  type BookBankFields,
  type SubmitState,
} from '@/components/SlipUpload';
import type { VisionModel } from '@/ai/loadVisionModels';
import { submitExpenseFromSlip } from '@/app/actions/expense';
import { useSecondaryLocale } from '@/components/i18n/SecondaryLocaleProvider';
import { NewWaybillPanel } from './NewWaybillPanel';
import { StaffSubmitHelper } from './StaffSubmitHelper';
import { T } from '@/components/i18n/T';

interface Props {
  currentUserId: number;
  initialModels: VisionModel[];
}

const EMPTY_BANK: BookBankFields = {
  bankName: '',
  bankBranch: '',
  accountNumber: '',
  accountName: '',
};

export function NewExpensePanel({ currentUserId, initialModels }: Props) {
  const locale = useSecondaryLocale();
  const [payment, setPayment] = useState<'cash' | 'credit_card' | 'transfer'>('cash');
  const [payeeType, setPayeeType] = useState<'employee' | 'vendor'>('employee');
  const [bookBankSlipId, setBookBankSlipId] = useState<number | null>(null);
  const [bookBankFields, setBookBankFields] = useState<BookBankFields>(EMPTY_BANK);
  const [submitState, setSubmitState] = useState<SubmitState | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const [submitError, setSubmitError] = useState<string | null>(null);

  const draft = submitState?.draft ?? null;
  const receiptSlipId = submitState?.slipId ?? null;
  const receiptHasFile = submitState?.pendingFile === true;
  const receiptReady = submitState?.canConfirm === true;
  const needsBookBank = payment === 'transfer';
  const bankReady = !needsBookBank || (
    bookBankSlipId != null
    && bookBankFields.bankName.trim().length > 0
    && bookBankFields.accountNumber.trim().length > 0
    && bookBankFields.accountName.trim().length > 0
  );
  const canSubmit = receiptReady && bankReady && !submitting;
  const total = draft?.totalAmount ?? 0;

  const blocker = !receiptHasFile
    ? { Icon: Upload, id: 'expense.blockerReceipt' }
    : submitState?.extractionState === 'running'
      ? { Icon: Loader2, id: 'expense.blockerReading' }
      : needsBookBank && !bankReady
        ? { Icon: Landmark, id: 'expense.blockerBank' }
        : !receiptReady
          ? { Icon: CircleAlert, id: 'expense.blockerFields' }
          : null;

  async function handleSubmit() {
    if (!receiptSlipId || !draft || !canSubmit) return;
    setSubmitError(null);
    setSubmitting(true);
    const result = await submitExpenseFromSlip({
      slipId: receiptSlipId,
      actorId: currentUserId,
      overrides: {
        vendorName: draft.vendorName,
        vendorAddress: draft.vendorAddress,
        createdTo: draft.createdTo,
        createdToAddress: draft.createdToAddress,
        transactionDate: draft.transactionDate,
        subtotal: draft.subtotal,
        vatAmount: draft.vatAmount,
        totalAmount: draft.totalAmount,
        paymentMethod: draft.paymentMethod,
        payeeType: draft.payeeType,
        items: draft.items,
        ...(needsBookBank && bookBankSlipId
          ? { bookBankSlipId, bookBankFields }
          : {}),
      },
    });
    setSubmitting(false);
    if (!result.success) {
      setSubmitError(result.error ?? 'Submit failed');
      return;
    }
    if (result.waybillId) {
      window.location.assign(`/waybill/${result.waybillId}?submitted=1`);
    } else if (result.expenseId) {
      window.location.assign(`/waybill/by-expense/${result.expenseId}?submitted=1`);
    }
  }

  const submitLabel = submitting ? (
    <span className="inline-flex items-center gap-2">
      <Loader2 className="size-4 animate-spin" aria-hidden />
      <T id="waybill.expense.saving" />
    </span>
  ) : (
    <span className="inline-flex items-center gap-2">
      {canSubmit ? <ArrowUpRight className="size-4" aria-hidden /> : <Lock className="size-4" aria-hidden />}
      <T id="waybill.expense.submitForApproval" />
    </span>
  );

  return (
    <NewWaybillPanel
      domain="expense"
      currentUserId={currentUserId}
      initialDraft={null}
      title=""
      titleTh=""
      discardLabel={null}
      submitLabel={submitLabel}
      readyToSubmit={canSubmit}
      submitting={submitting}
      onSubmit={handleSubmit}
      onDiscard={() => {}}
      hint={<T id="expense.composerHint" />}
      draftWaybillId={null}
      stickyActionBar={
        <div
          className="sticky bottom-24 z-10 -mx-5 rounded-xl border border-rule bg-paper-2/95 px-4 py-3 shadow-popover backdrop-blur sm:-mx-7 sm:px-5 md:bottom-2"
          data-testid="expense-sticky-bar"
        >
          <div className="flex flex-wrap items-center gap-3 sm:flex-nowrap">
            <div className="flex min-w-0 flex-1 items-center gap-2" aria-live="polite">
              {blocker ? (
                <>
                  <span className="grid size-10 shrink-0 place-items-center rounded-lg border border-caution/40 bg-caution-soft text-caution">
                    <blocker.Icon className={submitState?.extractionState === 'running' ? 'size-4 animate-spin' : 'size-4'} aria-hidden />
                  </span>
                  <span className="text-sm text-ink-2"><T id={blocker.id} /></span>
                </>
              ) : (
                <>
                  <span className="grid size-10 shrink-0 place-items-center rounded-lg border border-positive/40 bg-positive-soft text-positive">
                    <CircleCheck className="size-4" aria-hidden strokeWidth={2.5} />
                  </span>
                  <span className="text-sm text-positive-strong"><T id="expense.readyToSubmit" /></span>
                </>
              )}
            </div>
            <div className="ml-auto flex w-full items-center justify-between gap-3 sm:w-auto sm:justify-end">
              <div className="text-right">
                <div className="font-display text-2xl font-bold tabular-nums text-ink">
                  {total > 0 ? total.toLocaleString('th-TH', { minimumFractionDigits: 2 }) : '—'}
                </div>
                <div className="text-[10px] font-mono uppercase tracking-widest text-mute">THB</div>
              </div>
              <button
                type="button"
                onClick={handleSubmit}
                disabled={!canSubmit}
                data-testid="expense-sticky-submit"
                className={[
                  'inline-flex min-h-12 shrink-0 items-center justify-center gap-2 rounded-lg border-2 px-4 text-sm font-semibold transition disabled:cursor-not-allowed disabled:opacity-50',
                  canSubmit
                    ? 'action-button border-action bg-action text-action-ink hover:bg-action-hover'
                    : 'border-rule-strong bg-paper-3 text-mute',
                ].join(' ')}
              >
                {submitLabel}
              </button>
            </div>
          </div>
          {submitError && (
            <p role="alert" className="mt-2 flex items-center gap-1.5 text-xs text-critical" data-testid="expense-sticky-error">
              <CircleAlert className="size-3.5 shrink-0" aria-hidden />
              {submitError}
            </p>
          )}
        </div>
      }
    >
      <section className="space-y-4" aria-label="Receipt">
        <header className="flex items-start gap-3">
          <span className="grid size-10 shrink-0 place-items-center rounded-lg border border-accent/30 bg-accent-soft text-accent">
            <Receipt className="size-5" aria-hidden />
          </span>
          <div>
            <h3 className="text-base font-semibold text-ink"><T id="waybill.expense.upload_receipt" /></h3>
            <p className="mt-0.5 text-sm text-ink-2"><T id="expense.receiptHelp" /></p>
          </div>
        </header>
        <SlipUpload
          kind="receipt"
          currentUserId={currentUserId}
          initialModels={initialModels}
          bookBankSlipId={needsBookBank ? bookBankSlipId : null}
          bookBankFields={needsBookBank ? bookBankFields : undefined}
          payeeType={payeeType}
          onPaymentChange={setPayment}
          onSubmitStateChange={setSubmitState}
          hideSubmitButton
          draftWaybillId={null}
          onConfirmed={({ expenseId, waybillId }) => {
            if (waybillId) window.location.assign(`/waybill/${waybillId}?submitted=1`);
            else if (expenseId) window.location.assign(`/waybill/by-expense/${expenseId}?submitted=1`);
          }}
        />
      </section>

      <section className="rounded-xl border border-rule bg-paper-3/35 p-3 sm:p-4" aria-label="Payee type">
        <p className="mb-2 text-xs font-mono uppercase tracking-widest text-mute"><T id="expense.payeeLabel" /></p>
        <div className="grid grid-cols-2 gap-2">
          <button
            type="button"
            onClick={() => setPayeeType('employee')}
            aria-pressed={payeeType === 'employee'}
            className={[
              'flex min-h-12 items-center gap-2 rounded-lg border px-3 py-2.5 text-left text-sm transition',
              payeeType === 'employee' ? 'border-accent bg-accent-soft text-accent-strong' : 'border-rule bg-paper-2 text-ink-2',
            ].join(' ')}
          >
            <UserRound className="size-4 shrink-0" aria-hidden />
            <T id="expense.employeeReimbursement" hideSecondary />
          </button>
          <button
            type="button"
            onClick={() => setPayeeType('vendor')}
            aria-pressed={payeeType === 'vendor'}
            className={[
              'flex min-h-12 items-center gap-2 rounded-lg border px-3 py-2.5 text-left text-sm transition',
              payeeType === 'vendor' ? 'border-accent bg-accent-soft text-accent-strong' : 'border-rule bg-paper-2 text-ink-2',
            ].join(' ')}
          >
            <Building2 className="size-4 shrink-0" aria-hidden />
            <T id="expense.vendorPayment" hideSecondary />
          </button>
        </div>
      </section>

      <Activity mode={needsBookBank ? 'visible' : 'hidden'}>
        <section className="space-y-4 rounded-xl border border-info/35 bg-info-soft/20 p-4" aria-label="Add book bank slip" data-testid="expense-book-bank-section">
          <header className="flex items-start gap-3">
            <span className="grid size-10 shrink-0 place-items-center rounded-lg border border-info/35 bg-info-soft text-info">
              <Landmark className="size-5" aria-hidden />
            </span>
            <div>
              <h3 className="text-base font-semibold text-ink"><T id="expense.transferDetails" /></h3>
              <p className="mt-0.5 text-sm text-ink-2"><T id="expense.transferDetailsHint" /></p>
            </div>
          </header>
          <SlipUpload
            kind="book_bank"
            currentUserId={currentUserId}
            initialModels={initialModels}
            onSlipReady={(slipId, kind) => {
              if (kind === 'book_bank') setBookBankSlipId(slipId);
            }}
            onSlipDiscarded={(slipId, kind) => {
              if (kind === 'book_bank' && bookBankSlipId === slipId) {
                setBookBankSlipId(null);
                setBookBankFields(EMPTY_BANK);
              }
            }}
            onBookBankFieldsChange={setBookBankFields}
            hideSubmitButton
          />
        </section>
      </Activity>

      <details className="group rounded-xl border border-rule bg-paper-3/25">
        <summary className="flex min-h-12 cursor-pointer list-none items-center justify-between gap-3 px-4 py-3 text-sm font-semibold text-ink-2 [&::-webkit-details-marker]:hidden">
          <span><T id="expense.needWritingHelp" /></span>
          <ChevronDown className="size-4 text-mute transition-transform group-open:rotate-180" aria-hidden />
        </summary>
        <div className="border-t border-rule p-3">
          <StaffSubmitHelper currentUserId={currentUserId} lang={locale === 'th' ? 'th' : locale === 'de' ? 'de' : 'en'} />
        </div>
      </details>
    </NewWaybillPanel>
  );
}

export default NewExpensePanel;
