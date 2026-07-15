'use client';

import { useState } from 'react';
import {
  SlipUpload,
  type BookBankFields,
  type SubmitState,
} from '@/components/SlipUpload';
import type { VisionModel } from '@folio-lib/ai/loadVisionModels';
import { submitExpenseFromSlip } from '@/app/actions';
import { StepCard, StepBadge } from '@/components/StepCard';
import { useSecondaryLocale } from '@/components/i18n/SecondaryLocaleProvider';
import { Bilingual } from '@/components/i18n/Bilingual';
import { NewWaybillPanel } from './NewWaybillPanel';

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
      icon: '📤',
      title: 'Upload receipt & book bank',
      titleTh: 'อัพโหลดใบเสร็จและสมุดบัญชี',
      done: receiptReady && (needsBookBank ? bookBankReady : true),
      active: !receiptReady || (needsBookBank && !bookBankReady),
    },
    {
      key: 'review' as const,
      n: 2,
      icon: '🚀',
      title: 'Review & submit',
      titleTh: 'ตรวจสอบและส่ง',
      done: canSubmitAll,
      active: canSubmitAll,
    },
  ];

  const completedCount = steps.filter((s) => s.done).length;

  async function handleSubmitAll() {
    if (!receiptSlipId) return;
    setSubmitting(true);
    await submitExpenseFromSlip({
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
  }

  const hint = (
    <Bilingual
      en="Upload a receipt → AI reads the slip → pick a payment method → submit for approval. About 1–2 minutes."
      th="อัพโหลดใบเสร็จ → AI อ่านสลิป → เลือกวิธีชำระ → ส่งเพื่ออนุมัติ ใช้เวลาประมาณ 1–2 นาที"
      locale={locale}
    />
  );

  const submitLabel = submitting
    ? <Bilingual en="⏳ Saving…" th="⏳ กำลังบันทึก…" locale={locale} />
    : canSubmitAll
      ? <Bilingual en="✓ Submit expense for approval" th="✓ ส่งค่าใช้จ่ายเพื่ออนุมัติ" locale={locale} />
      : <Bilingual en="🔒 Submit (disabled)" th="🔒 ส่ง (ปิดอยู่)" locale={locale} />;

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
    >
      <ol
        className="grid grid-cols-1 sm:grid-cols-3 gap-2"
        aria-label="Progress steps"
      >
        {steps.map((s) => (
          <li
            key={s.key}
            className={[
              'flex items-center gap-3 rounded-xl border px-3 py-2.5 transition-colors',
              s.done
                ? 'glass-tint-positive border-positive/40'
                : s.active
                ? 'glass-tint-info border-info/40'
                : 'glass-panel border-rule',
            ].join(' ')}
          >
            <StepBadge n={s.n} done={s.done} active={s.active} />
            <div className="min-w-0">
              <div className="flex items-center gap-1.5">
                <span aria-hidden className="text-sm">{s.icon}</span>
                <p className="text-xs font-bold text-white truncate">{s.title}</p>
              </div>
              <p className="text-sm font-mono text-mute truncate">{s.titleTh}</p>
            </div>
          </li>
        ))}
      </ol>

      <div className="mt-3 flex items-center gap-3">
        <div className="glass-panel flex-1 h-1.5 rounded-full overflow-hidden">
          <div
            className="h-full bg-accent transition-[width] duration-500 ease-out"
            style={{ width: `${(completedCount / steps.length) * 100}%` }}
            aria-hidden
          />
        </div>
        <span className="text-sm font-mono text-ink-2 tabular-nums">
          {completedCount}/{steps.length}
        </span>
      </div>

      <StepCard
        n={1}
        icon="📸"
        title="Upload receipt & book bank"
        titleTh="อัพโหลดใบเสร็จและสมุดบัญชี"
        hint="Drop a photo, scan, or PDF of the receipt. For transfer payments, also drop a book bank slip."
        done={receiptReady && (needsBookBank ? bookBankReady : true)}
        active={!receiptReady || (needsBookBank && !bookBankReady)}
        tone="indigo"
        badge={
          receiptHasFile ? (
            <span className="glass-tint-info text-sm font-mono px-2 py-0.5 rounded-full text-info">
              {receiptReady
                ? '✓ OCR done · ✓ OCR เสร็จ'
                : 'OCR in progress… · OCR กำลังทำงาน…'}
            </span>
          ) : null
        }
      >
        <div className="space-y-6">
          <div>
            <h4 className="mb-2 flex items-center gap-2 text-xs font-bold text-ink-2 font-mono">
              <span>📄</span>
              Receipt · ใบเสร็จ
            </h4>
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
          {needsBookBank && (
            <div>
              <h4 className="mb-2 flex items-center gap-2 text-xs font-bold text-ink-2 font-mono">
                <span>🏦</span>
                Book bank · สมุดบัญชี
                {bookBankSlipId != null && (
                  <span className="glass-tint-positive px-2 py-0.5 rounded-full text-positive text-sm font-mono">
                    {`SLIP-${bookBankSlipId} attached`}
                  </span>
                )}
              </h4>
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
          )}
        </div>
      </StepCard>
    </NewWaybillPanel>
  );
}

export default NewExpensePanel;
