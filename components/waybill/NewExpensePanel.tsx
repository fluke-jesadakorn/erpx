'use client';

import { useState } from 'react';
import {
  Upload,
  Rocket,
  Banknote,
  CreditCard,
  CircleDot,
  CircleAlert,
  ArrowRight,
  ArrowUpRight,
  Loader2,
  Lock,
  CircleCheck,
  Wallet,
  Building2,
  Landmark,
  Receipt,
} from 'lucide-react';
import {
  SlipUpload,
  type BookBankFields,
  type SubmitState,
} from '@/components/SlipUpload';
import type { VisionModel } from '@/ai/loadVisionModels';
import { submitExpenseFromSlip } from '@/app/actions/expense';
import { StepCard, StepBadge } from '@/components/StepCard';
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
  const [bookBankSlipId, setBookBankSlipId] = useState<number | null>(null);
  const [bookBankFields, setBookBankFields] = useState<BookBankFields>(EMPTY_BANK);
  const [submitState, setSubmitState] = useState<SubmitState | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const [submitError, setSubmitError] = useState<string | null>(null);

  void submitExpenseFromSlip;

  const parsed = submitState?.parsed ?? null;
  const receiptSlipId = submitState?.slipId ?? null;
  const receiptReady = submitState?.canConfirm === true;
  const receiptHasFile = submitState?.pendingFile === true;
  const needsBookBank = payment === 'transfer';
  const bookBankReady: boolean =
    !needsBookBank ||
    (bookBankSlipId != null &&
      bookBankFields.bankName.length > 0 &&
      bookBankFields.accountNumber.length > 0 &&
      bookBankFields.accountName.length > 0);
  const canSubmitAll = receiptReady && bookBankReady && !submitting;

  const steps = [
    {
      key: 'upload' as const,
      n: 1,
      Icon: Upload,
      title: 'Upload receipt',
      titleTh: 'อัพโหลดใบเสร็จ',
      done: receiptReady && (needsBookBank ? bookBankReady : true),
      active: !receiptReady || (needsBookBank && !bookBankReady),
    },
    {
      key: 'review' as const,
      n: 2,
      Icon: Rocket,
      title: 'Review',
      titleTh: 'ตรวจสอบ',
      done: canSubmitAll,
      active: canSubmitAll,
    },
  ];

  const completedCount = steps.filter((s) => s.done).length;
  const receiptTotal = parsed?.totalAmount ?? 0;
  const blocker = !receiptHasFile
    ? { Icon: Upload, en: 'Drop a receipt', th: 'อัพโหลดใบเสร็จ' }
    : !receiptReady
    ? { Icon: Loader2, en: 'Wait for OCR', th: 'รอ OCR' }
    : needsBookBank && !bookBankReady
    ? { Icon: Building2, en: 'Add book bank slip', th: 'เพิ่มสลิปสมุดบัญชี' }
    : null;

  const PAYMENT_LABEL: Record<typeof payment, { en: string; th: string; Icon: typeof Wallet }> = {
    cash: { en: 'Cash', th: 'เงินสด', Icon: Wallet },
    credit_card: { en: 'Card', th: 'บัตรเครดิต', Icon: CreditCard },
    transfer: { en: 'Transfer', th: 'โอนเงิน', Icon: Building2 },
  };
  const payMeta = PAYMENT_LABEL[payment];

  function scrollToStep(n: number) {
    if (typeof document === 'undefined') return;
    document.getElementById(`step-${n}`)?.scrollIntoView({
      behavior: 'smooth',
      block: 'start',
    });
  }

  async function handleSubmitAll() {
    if (!receiptSlipId) return;
    setSubmitError(null);
    setSubmitting(true);
    const r = await submitExpenseFromSlip({
      slipId: receiptSlipId,
      actorId: currentUserId,
      overrides: needsBookBank && bookBankSlipId
        ? {
            paymentMethod: 'transfer',
            bookBankSlipId,
            bookBankFields,
          }
        : { paymentMethod: payment },
    });
    setSubmitting(false);
    if (!r.success) {
      setSubmitError(r.error ?? 'Submit failed');
      return;
    }
    if (r.waybillId) {
      window.location.assign(`/waybill/${r.waybillId}`);
    } else if (r.expenseId) {
      window.location.assign(`/waybill/by-expense/${r.expenseId}`);
    }
  }

  const hint = (
    <span title="Upload → AI reads → submit · อัพโหลด → AI อ่าน → ส่ง">
      <T id="waybill.expense.upload_ai_reads_submit" />
    </span>
  );

  const submitLabel = submitting ? (
    <span className="inline-flex items-center gap-2">
      <Loader2 className="size-4 animate-spin" aria-hidden />
      <T id="waybill.expense.saving" />
    </span>
  ) : canSubmitAll ? (
    <span className="inline-flex items-center gap-2">
      <ArrowRight className="size-4" aria-hidden />
      <T id="waybill.expense.submit" />
    </span>
  ) : (
    <span className="inline-flex items-center gap-2">
      <Lock className="size-4" aria-hidden />
      <T id="waybill.expense.submit" />
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
      readyToSubmit={canSubmitAll}
      submitting={submitting}
      onSubmit={handleSubmitAll}
      onDiscard={() => {}}
      hint={hint}
      draftWaybillId={null}
      headerExtra={
        <span
          aria-live="polite"
          title={
            receiptTotal > 0
              ? `${receiptTotal.toLocaleString('th-TH', { minimumFractionDigits: 2 })} THB`
              : '—'
          }
          className={[
            'inline-flex items-center gap-1.5 rounded-md border px-2.5 py-0.5 text-sm font-semibold font-sans tabular-nums tabular-nums',
            canSubmitAll
              ? 'border-positive/40 bg-positive-soft text-positive'
              : receiptTotal > 0
              ? 'border-info/40 bg-info-soft text-info'
              : 'border-rule bg-paper-3 text-mute',
          ].join(' ')}
          data-testid="expense-header-total"
        >
          <Banknote className="size-3.5" aria-hidden strokeWidth={2} />
          {receiptTotal > 0
            ? `${receiptTotal.toLocaleString('th-TH', { minimumFractionDigits: 2 })} THB`
            : '— THB'}
        </span>
      }
      stickyActionBar={
        <div
          className="sticky bottom-2 z-10 -mx-5 sm:-mx-7 px-5 sm:px-7 py-4 glass-panel-heavy rounded-xl border border-rule border-l-4 border-l-accent/60 shadow-popover"
          data-testid="expense-sticky-bar"
        >
          <div className="grid grid-cols-1 sm:grid-cols-[minmax(0,1fr)_auto] items-center gap-3 sm:gap-5">
            <div className="flex min-w-0 items-center gap-2 text-sm">
              {blocker ? (
                <span
                  title={`${blocker.en} · ${blocker.th}`}
                  aria-label={blocker.en}
                  className="inline-flex shrink-0 w-11 h-11 items-center justify-center rounded-lg bg-caution-soft text-caution border border-caution/40"
                >
                  <blocker.Icon className="size-4" strokeWidth={2} aria-hidden />
                </span>
              ) : (
                <>
                  <span
                    aria-label="Ready"
                    title="Ready · พร้อม"
                    className="inline-flex shrink-0 w-11 h-11 items-center justify-center rounded-lg bg-positive-soft text-positive border border-positive/40"
                  >
                    <CircleCheck className="size-4" strokeWidth={2.5} aria-hidden />
                  </span>
                  <div className="flex items-center gap-1.5 min-w-0 flex-wrap">
                    <span
                      title={`${payMeta.en} · ${payMeta.th}`}
                      className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-md border border-rule bg-paper-3 text-sm font-medium font-sans tabular-nums text-ink"
                    >
                      <payMeta.Icon className="size-3" aria-hidden strokeWidth={2} />
                      {payMeta.en}
                    </span>
                    {parsed?.vendorName && (
                      <span
                        title={`Vendor · ผู้ขาย`}
                        className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-md border border-rule bg-paper-3 text-sm font-medium font-sans text-ink max-w-[180px] truncate"
                      >
                        <Building2 className="size-3" aria-hidden strokeWidth={2} />
                        {parsed.vendorName}
                      </span>
                    )}
                  </div>
                </>
              )}
            </div>
            <div className="hidden sm:block w-px h-11 bg-rule" aria-hidden />
            <div className="flex items-center gap-4 justify-self-stretch sm:justify-self-end">
              <div
                aria-live="polite"
                title={
                  receiptTotal > 0
                    ? `${receiptTotal.toLocaleString('th-TH', { minimumFractionDigits: 2 })} THB · รวม`
                    : '—'
                }
                className={[
                  'font-display text-3xl font-bold tabular-nums leading-none tracking-tight',
                  canSubmitAll ? 'text-positive' : 'text-ink',
                ].join(' ')}
                data-testid="expense-sticky-total"
              >
                {receiptTotal > 0
                  ? receiptTotal.toLocaleString('th-TH', { minimumFractionDigits: 2 })
                  : '—'}
                <span className="ml-1.5 text-base font-medium font-sans text-ink-2 uppercase tracking-wider">THB</span>
              </div>
              <button
                type="button"
                onClick={handleSubmitAll}
                disabled={!canSubmitAll}
                title={
                  canSubmitAll
                    ? 'Submit for approval · ส่งเพื่ออนุมัติ'
                    : 'Disabled · ปิดอยู่'
                }
                data-testid="expense-sticky-submit"
                className={[
                  'shrink-0 inline-flex items-center justify-center gap-2 rounded-lg border-2 w-12 h-12 transition-all duration-200',
                  'disabled:opacity-50 disabled:cursor-not-allowed',
                  submitting
                    ? 'bg-rule-strong text-ink-2 border-rule-strong'
                    : canSubmitAll
                    ? 'bg-accent hover:bg-accent-strong text-paper-2 border-accent shadow-[0_8px_24px_-8px_color-mix(in_oklab,var(--accent)_55%,transparent)]'
                    : 'bg-paper-3 text-mute border-rule-strong',
                  'ring-1 ring-accent/30',
                ].join(' ')}
              >
                {submitting ? (
                  <Loader2 className="size-5 animate-spin" aria-hidden />
                ) : canSubmitAll ? (
                  <ArrowUpRight className="size-5" aria-hidden strokeWidth={2.5} />
                ) : (
                  <Lock className="size-5" aria-hidden strokeWidth={2} />
                )}
                <span className="sr-only">
                  {canSubmitAll ? 'Submit' : 'Submit (locked)'}
                </span>
              </button>
            </div>
          </div>
          {submitError && (
            <p
              role="alert"
              title={submitError}
              data-testid="expense-sticky-error"
              className="mt-2 flex items-center gap-1.5 text-xs text-critical font-sans tabular-nums"
            >
              <CircleAlert className="size-3.5 shrink-0" strokeWidth={2.5} aria-hidden />
              <span className="truncate">{submitError}</span>
            </p>
          )}
        </div>
      }
    >
      <div className="flex items-center justify-end" aria-label="Progress">
        <span className="text-xs text-mute font-sans tabular-nums">
          {completedCount}/{steps.length}
        </span>
      </div>
      <ol className="relative grid grid-cols-2 gap-y-3">
        {steps.map((s, i) => {
          const Icon = s.Icon;
          return (
            <li key={s.key} className="relative flex justify-center">
              {i < steps.length - 1 && (
                <span
                  aria-hidden
                  className="hidden sm:block absolute top-[18px] left-[calc(50%+22px)] right-[calc(-50%+22px)] h-px bg-rule overflow-hidden"
                >
                  <span
                    className="block h-full bg-positive transition-[width] duration-500 ease-out"
                    style={{ width: completedCount > i ? '100%' : '0%' }}
                  />
                </span>
              )}
              <button
                type="button"
                onClick={() => scrollToStep(s.n)}
                aria-current={s.active ? 'step' : undefined}
                aria-label={`Jump to step ${s.n}: ${s.title}`}
                title={`${s.title} · ${s.titleTh}`}
                className="group flex flex-col items-center gap-1.5 w-full sm:w-auto px-2 py-2 transition-colors"
              >
                <StepBadge n={s.n} done={s.done} active={s.active} tone="accent" />
                <Icon
                  aria-hidden
                  className={[
                    'size-4 mt-0.5',
                    s.done
                      ? 'text-positive'
                      : s.active
                      ? 'text-accent'
                      : 'text-mute',
                  ].join(' ')}
                  strokeWidth={2}
                />
                <p
                  className={[
                    'text-sm font-semibold text-center',
                    s.done || s.active ? 'text-ink font-semibold' : 'text-ink-2',
                  ].join(' ')}
                >
                  {s.title}
                </p>
              </button>
            </li>
          );
        })}
      </ol>

      <StepCard
        n={1}
        icon={<Upload className="size-4" strokeWidth={2} aria-hidden />}
        title={
          <T id="waybill.expense.upload_receipt" />
        }
        hint={
          <span title="Drop a photo, scan, or PDF · ลาก/วางไฟล์รูป สแกน หรือ PDF">
            <T id="waybill.expense.photo_scan_or_pdf" />
          </span>
        }
        cardId="step-1"
        done={receiptReady && (needsBookBank ? bookBankReady : true)}
        active={!receiptReady || (needsBookBank && !bookBankReady)}
        tone="accent"
        bodyTint
        flat
        badge={
          receiptHasFile ? (
            <span className="text-xs font-sans tabular-nums px-2 py-0.5 rounded-full border border-info/40 bg-info-soft text-info inline-flex items-center gap-1">
              {receiptReady ? (
                <>
                  <CircleCheck className="size-3" aria-hidden strokeWidth={2.5} />
                  <span title="OCR done · OCR เสร็จ">OCR</span>
                </>
              ) : (
                <>
                  <Loader2 className="size-3 animate-spin" aria-hidden />
                  <span title="OCR in progress · OCR กำลังทำงาน">OCR</span>
                </>
              )}
            </span>
          ) : null
        }
      >
        <div className="grid grid-cols-1 md:grid-cols-2 gap-5">
          <div className="rounded-xl border border-rule bg-paper-2/40 p-5 space-y-4">
            <header className="flex items-start justify-between gap-3 pb-3 border-b border-rule">
              <div className="flex items-center gap-2.5 min-w-0">
                <Receipt className="size-5 text-accent shrink-0" aria-hidden strokeWidth={2} />
                <div className="min-w-0">
                  <p className="text-sm font-semibold text-ink leading-tight"><T id="waybill.expense.receipt" /></p>
                  <p className="text-xs text-mute mt-0.5 font-sans tabular-nums uppercase tracking-wider">RECEIPT</p>
                </div>
              </div>
              {receiptHasFile ? (
                <span className="inline-flex items-center gap-1 rounded-md border border-info/40 bg-info-soft px-2.5 py-1 text-xs font-bold font-sans tabular-nums uppercase tracking-widest text-info">
                  {receiptReady ? (
                    <CircleCheck className="size-3" aria-hidden strokeWidth={2.5} />
                  ) : (
                    <Loader2 className="size-3 animate-spin" aria-hidden />
                  )}
                  {receiptReady
                    ? <T id="waybill.expense.ocr_ok" />
                    : <T id="waybill.expense.ocr" />}
                </span>
              ) : (
                <span className="inline-flex items-center gap-1 rounded-md border border-rule bg-paper-3 px-2.5 py-1 text-xs font-bold font-sans tabular-nums uppercase tracking-widest text-mute">
                  <CircleDot className="size-3" aria-hidden strokeWidth={2} />
                  <T id="waybill.expense.empty" />
                </span>
              )}
            </header>
            <SlipUpload
              kind="receipt"
              currentUserId={currentUserId}
              initialModels={initialModels}
              bookBankSlipId={needsBookBank ? bookBankSlipId : null}
              bookBankFields={needsBookBank ? bookBankFields : undefined}
              onPaymentChange={setPayment}
              onSubmitStateChange={setSubmitState}
              hideSubmitButton
              draftWaybillId={null}
              onConfirmed={({ expenseId, waybillId }) => {
                if (waybillId) window.location.assign(`/waybill/${waybillId}`);
                else if (expenseId) window.location.assign(`/waybill/by-expense/${expenseId}`);
              }}
            />
          </div>

          <div className="rounded-xl border border-rule bg-paper-2/40 p-5 space-y-4">
            <header className="flex items-start justify-between gap-3 pb-3 border-b border-rule">
              <div className="flex items-center gap-2.5 min-w-0">
                <Landmark className="size-5 text-accent shrink-0" aria-hidden strokeWidth={2} />
                <div className="min-w-0">
                  <p className="text-sm font-semibold text-ink leading-tight"><T id="waybill.expense.book_bank" /></p>
                  <p className="text-xs text-mute mt-0.5 font-sans tabular-nums uppercase tracking-wider">BOOK BANK</p>
                </div>
              </div>
              {bookBankSlipId != null ? (
                <span className="inline-flex items-center gap-1 rounded-md border border-positive/40 bg-positive-soft px-2.5 py-1 text-xs font-bold font-sans tabular-nums uppercase tracking-widest text-positive-strong">
                  <CircleCheck className="size-3" aria-hidden strokeWidth={2.5} />
                  SLIP-{bookBankSlipId}
                </span>
              ) : (
                <span className="inline-flex items-center gap-1 rounded-md border border-rule bg-paper-3 px-2.5 py-1 text-xs font-bold font-sans tabular-nums uppercase tracking-widest text-mute">
                  <CircleDot className="size-3" aria-hidden strokeWidth={2} />
                  <T id="waybill.expense.empty" />
                </span>
              )}
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
          </div>
        </div>
      </StepCard>

      <StaffSubmitHelper currentUserId={currentUserId} lang={locale === 'th' ? 'th' : 'en'} />
    </NewWaybillPanel>
  );
}

export default NewExpensePanel;
