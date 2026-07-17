'use client';

import React, { forwardRef, useEffect, useImperativeHandle, useState } from 'react';
import {
  Building2,
  Upload,
  UploadCloud,
  RefreshCw,
  Trash2,
  Loader2,
  CircleDot,
  Banknote,
  Hash,
  User,
  Calendar,
  CircleCheck,
  CircleAlert,
  ZoomIn,
  FileSpreadsheet,
  Wand2,
  ArrowUpRight,
  Lock,
  Receipt as ReceiptIcon,
} from 'lucide-react';
import { Modal } from '@/components/ui/Modal';
import {
  FilledTick,
  Field,
  FieldSpinner,
  SectionHeader,
  fileKind,
  formatBytes,
} from '@/components/slips/SlipCard';
import { ModelSelect } from '@/components/slips/ModelSelect';
import { useSlipOcr } from '@/components/slips/useSlipOcr';
import type { ParsedFields } from '@/components/slips/types';
import { SlipThumbZoom } from './SlipThumbZoom';
import { attachSalesPaymentSlipAction } from '@/app/actions/sales';

export interface SalesPaymentSlipUploadHandle {
  open: () => void;
}

export interface SalesPaymentSlipUploadProps {
  waybillId: string;
  soId: number;
  locale?: 'en' | 'th';
  currentUserId?: number;
  initialModels?: Parameters<typeof useSlipOcr>[0]['initialModels'];
  existingSlipId?: number | null;
  existingSlipUrl?: string | null;
  existingSlipName?: string | null;
  onAttached?: (info: { slipId: number; soId: number; waybillId: string }) => void;
}

const INPUT_CLS =
  'w-full rounded-xl border border-rule bg-paper px-3.5 py-2.5 pr-9 text-xs text-ink transition-colors hover:bg-paper-3 focus:border-positive focus:ring-2 focus:ring-positive/20 focus:outline-none disabled:cursor-not-allowed disabled:opacity-50 placeholder:text-mute font-medium';

const NUMBER_CLS =
  'w-full rounded-xl border border-rule bg-paper px-3.5 py-2.5 pr-9 text-xs text-ink font-mono text-right transition-colors hover:bg-paper-3 focus:border-positive focus:ring-2 focus:ring-positive/20 focus:outline-none disabled:cursor-not-allowed disabled:opacity-50 placeholder:text-mute font-medium';

function t(key: string, locale: 'en' | 'th' | undefined): string {
  const th: Record<string, string> = {
    upload: 'อัพโหลดสลิปการชำระเงิน',
    drop: 'วางไฟล์หรือคลิก',
    hint: 'JPG · PNG · WEBP · PDF · ≤ 20 MB',
    attached: 'สลิปแนบแล้ว',
    confirm: 'ยืนยันการรับชำระ',
    errorNoSlip: 'กรุณาอัพโหลดสลิปก่อน',
    errorNoAmount: 'กรุณาระบุจำนวนเงิน',
    errorNoDate: 'กรุณาระบุวันที่ทำรายการ',
    attachedOk: 'แนบสลิปเรียบร้อย',
    ready: 'พร้อมยืนยัน',
    needFields: 'กรอกข้อมูลให้ครบ',
  };
  if (locale === 'th') return th[key] ?? key;
  const en: Record<string, string> = {
    upload: 'Upload payment slip',
    drop: 'Drop or click',
    hint: 'JPG · PNG · WEBP · PDF · ≤ 20 MB',
    attached: 'Slip attached',
    confirm: 'Confirm payment received',
    errorNoSlip: 'Upload a slip first',
    errorNoAmount: 'Amount required',
    errorNoDate: 'Transaction date required',
    attachedOk: 'Slip attached',
    ready: 'Ready to confirm',
    needFields: 'Fill all fields',
  };
  return en[key] ?? key;
}

export const SalesPaymentSlipUpload = forwardRef<
  SalesPaymentSlipUploadHandle,
  SalesPaymentSlipUploadProps
>(function SalesPaymentSlipUpload(
  {
    waybillId,
    soId,
    locale = 'en',
    currentUserId,
    initialModels,
    existingSlipId = null,
    existingSlipUrl = null,
    existingSlipName = null,
    onAttached,
  },
  ref,
) {
  const ocr = useSlipOcr({
    kind: 'book_bank',
    initialModels,
    currentUserId,
  });

  const [payerBankName, setPayerBankName] = useState('');
  const [payerAccountNumber, setPayerAccountNumber] = useState('');
  const [payerAccountName, setPayerAccountName] = useState('');
  const [receiverBankName, setReceiverBankName] = useState('');
  const [receiverAccountNumber, setReceiverAccountNumber] = useState('');
  const [receiverAccountName, setReceiverAccountName] = useState('');
  const [amount, setAmount] = useState('');
  const [transactionDate, setTransactionDate] = useState('');
  const [confirming, setConfirming] = useState(false);
  const [confirmError, setConfirmError] = useState<string | null>(null);
  const [dragOver, setDragOver] = useState(false);

  function applyParsed(p: ParsedFields) {
    setReceiverBankName(p.bankName ?? '');
    setReceiverAccountNumber(p.accountNumber ?? '');
    setReceiverAccountName(p.accountName ?? '');
  }

  useEffect(() => {
    if (ocr.parsed) applyParsed(ocr.parsed);
  }, [ocr.parsed]);

  useImperativeHandle(ref, () => ({
    open: () => ocr.inputRef.current?.click(),
  }));

  const disabled = !ocr.pendingFile || ocr.phase === 'confirming' || confirming;
  const confPct = Math.round(ocr.confidence * 100);
  const pendingKind = ocr.pendingFile
    ? fileKind(ocr.pendingFile.type, ocr.pendingFile.name)
    : '';

  const amountN = Number(amount);
  const dateOk = transactionDate.length > 0 && !Number.isNaN(new Date(transactionDate).getTime());
  const canConfirm =
    ocr.slipId != null &&
    ocr.extractionState === 'done' &&
    amount.trim().length > 0 &&
    !Number.isNaN(amountN) &&
    amountN > 0 &&
    dateOk;

  async function handleConfirm() {
    if (ocr.slipId == null) {
      setConfirmError(t('errorNoSlip', locale));
      return;
    }
    if (!amount || Number.isNaN(amountN) || amountN <= 0) {
      setConfirmError(t('errorNoAmount', locale));
      return;
    }
    if (!dateOk) {
      setConfirmError(t('errorNoDate', locale));
      return;
    }
    setConfirmError(null);
    setConfirming(true);
    try {
      await attachSalesPaymentSlipAction({
        waybillId,
        soId,
        slipId: ocr.slipId,
        fields: {
          payerBankName: payerBankName.trim() || undefined,
          payerAccountNumber: payerAccountNumber.trim() || undefined,
          payerAccountName: payerAccountName.trim() || undefined,
          receiverBankName: receiverBankName.trim() || undefined,
          receiverAccountNumber: receiverAccountNumber.trim() || undefined,
          receiverAccountName: receiverAccountName.trim() || undefined,
          amount: amountN,
          transactionDate: transactionDate || undefined,
        },
      });
      ocr.setPhase('confirmed');
      onAttached?.({ slipId: ocr.slipId, soId, waybillId });
    } catch (err) {
      setConfirmError(err instanceof Error ? err.message : String(err));
      setConfirming(false);
    }
  }

  const hasExisting = existingSlipId != null && existingSlipId > 0;

  if (hasExisting) {
    return (
      <div className="space-y-3 rounded-xl border border-positive/30 bg-positive-soft/30 p-3.5">
        <div className="flex items-center gap-2 flex-wrap">
          <span className="px-2 py-0.5 rounded-full text-xs font-mono inline-flex items-center gap-1 bg-positive-soft text-positive border border-positive/40">
            <CircleCheck className="size-3" strokeWidth={2.5} aria-hidden />
            {t('attached', locale)}
          </span>
          <span className="text-xs font-mono text-ink-2 inline-flex items-center gap-1">
            <ReceiptIcon className="size-3" aria-hidden strokeWidth={2} />
            SLIP-{existingSlipId}
          </span>
        </div>
        <div className="flex items-start gap-4">
          {existingSlipUrl ? (
            <SlipThumbZoom
              href={existingSlipUrl}
              alt={`SLIP-${existingSlipId}`}
              title={existingSlipName ?? `SLIP-${existingSlipId}`}
              subtitle={`SLIP-${existingSlipId}`}
              className="h-20 w-16"
            />
          ) : (
            <div className="h-20 w-16 rounded-lg flex items-center justify-center border border-rule bg-paper-3">
              <ReceiptIcon className="size-8 text-ink-2" strokeWidth={1.5} aria-hidden />
            </div>
          )}
          <div className="flex-1 min-w-0">
            <p className="text-sm font-bold text-ink truncate">
              {existingSlipName ?? `SLIP-${existingSlipId}`}
            </p>
            <p className="text-xs text-ink-2 font-mono">SO-{soId}</p>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="space-y-3">
      <input
        ref={ocr.inputRef}
        type="file"
        accept="image/*,application/pdf"
        onChange={ocr.onPick}
        className="hidden"
      />

      <div className="space-y-3">
        <div className="flex items-start gap-4">
          {!ocr.pendingFile ? (
            <div
              onDragOver={(e) => {
                e.preventDefault();
                setDragOver(true);
              }}
              onDragLeave={() => setDragOver(false)}
              onDrop={(e) => {
                setDragOver(false);
                ocr.onDrop(e);
              }}
              onClick={() => ocr.inputRef.current?.click()}
              className={`shrink-0 w-full h-32 sm:h-44 rounded-xl border-2 border-dashed flex flex-col items-center justify-center gap-1.5 cursor-pointer transition-colors ${
                dragOver
                  ? 'border-positive'
                  : 'border-rule-strong bg-paper-3/40 hover:border-positive/50'
              }`}
              data-testid="sales-payment-drop-zone"
              aria-label="Drop a customer payment slip or click to browse"
            >
              <UploadCloud className="size-7 text-ink-2" strokeWidth={1.5} aria-hidden />
              <p className="text-xs font-mono text-ink-2 text-center px-3">
                {t('drop', locale)}
              </p>
              <p className="text-xs font-mono text-mute text-center px-3">
                {t('hint', locale)}
              </p>
            </div>
          ) : ocr.preview ? (
            <button
              type="button"
              onClick={() => ocr.setZoomOpen(true)}
              disabled={ocr.extractionState === 'running'}
              className="relative shrink-0 group rounded-xl hover:border-positive/50 transition-colors disabled:cursor-default disabled:hover:border-rule border border-rule bg-paper-3"
              title={ocr.extractionState === 'running' ? '' : 'Click to enlarge'}
              data-testid="sales-payment-preview-zoom"
            >
              <img
                src={ocr.preview}
                alt="preview"
                className={`object-contain rounded-xl ${
                  ocr.extractionState === 'running' ? 'w-32 h-40' : 'w-40 h-52'
                }`}
              />
              {ocr.extractionState !== 'running' && (
                <span
                  aria-hidden
                  className="absolute bottom-1.5 right-1.5 grid place-items-center w-6 h-6 rounded-full ring-1 ring-rule-strong text-sm text-ink opacity-0 group-hover:opacity-100 transition-opacity bg-paper-2"
                >
                  <ZoomIn className="size-3.5" strokeWidth={2} />
                </span>
              )}
            </button>
          ) : (
            <div
              className={`shrink-0 rounded-xl flex items-center justify-center border border-rule bg-paper-3 ${
                ocr.extractionState === 'running' ? 'w-32 h-40' : 'w-40 h-52'
              }`}
            >
              <FileSpreadsheet className="size-12 text-ink-2" strokeWidth={1.2} aria-hidden />
            </div>
          )}

          {ocr.pendingFile && (
            <div className="flex-1 min-w-0 space-y-2">
              <div className="flex items-center gap-2 flex-wrap">
                <span
                  className={`px-2 py-0.5 rounded-full text-xs font-mono uppercase inline-flex items-center gap-1 ${
                    ocr.phase === 'confirmed'
                      ? 'bg-positive-soft text-positive border border-positive/40'
                      : ocr.extractionState === 'pending'
                        ? 'bg-rule-strong/40 text-ink-2'
                        : ocr.extractionState === 'running'
                          ? 'bg-caution-soft text-caution border border-caution/40'
                          : 'bg-positive-soft text-positive border border-positive/40'
                  }`}
                  data-testid="sales-payment-step-badge"
                >
                  {ocr.phase === 'confirmed' ? (
                    <CircleCheck className="size-3" strokeWidth={2.5} />
                  ) : ocr.extractionState === 'running' ? (
                    <Loader2 className="size-3 animate-spin" strokeWidth={2.5} />
                  ) : ocr.extractionState === 'done' && canConfirm ? (
                    <CircleCheck className="size-3" strokeWidth={2.5} />
                  ) : (
                    <span className="font-mono">2/2</span>
                  )}
                  {ocr.phase === 'confirmed'
                    ? 'ok'
                    : ocr.extractionState === 'running' && ocr.elapsed > 0
                      ? <span className="tabular-nums">{ocr.elapsed}s</span>
                      : ocr.extractionState === 'pending'
                        ? 'ready'
                        : ocr.extractionState === 'running'
                          ? 'OCR'
                          : 'review'}
                </span>
                <span className="text-xs font-mono text-ink-2 inline-flex items-center gap-1">
                  <FileSpreadsheet className="size-3" aria-hidden strokeWidth={2} />
                  {pendingKind} · {formatBytes(ocr.pendingFile.size)}
                </span>
                {ocr.extractionState === 'done' && (
                  <span
                    title={`${confPct}% confidence · mode ${ocr.mode}`}
                    className="text-xs font-mono text-ink-2"
                  >
                    <CircleDot className="size-3 inline-block mr-0.5 text-positive" strokeWidth={2.5} />
                    {confPct}%
                  </span>
                )}
              </div>
              <p className="text-sm font-bold text-ink truncate" title={ocr.pendingFile.name}>
                {ocr.pendingFile.name}
              </p>

              {ocr.extractionState === 'running' && (
                <div className="space-y-2">
                  <p className="text-xs text-ink-2 font-mono inline-flex items-center gap-1.5">
                    <Loader2 className="size-3 animate-spin" aria-hidden />
                    <span>
                      Running <span className="text-ink">{ocr.selectedModel || '…'}</span>
                      {ocr.elapsed >= 5 && (
                        <span className="text-mute"> · 1–3 min possible</span>
                      )}
                    </span>
                  </p>
                  <div className="relative w-full h-1.5 rounded-full overflow-hidden bg-rule">
                    <div
                      className="absolute inset-y-0 left-0 bg-positive/40 w-1/3 rounded-full"
                      style={{ animation: 'slip-indeterminate 1.4s ease-in-out infinite' }}
                    />
                  </div>
                </div>
              )}
            </div>
          )}
        </div>

        {ocr.pendingFile && ocr.extractionState !== 'running' && ocr.visionModels.length > 0 && (
          <div className="flex flex-wrap items-center gap-2 pt-2 border-t border-rule">
            <ModelSelect
              models={ocr.visionModels}
              value={ocr.selectedModel}
              onChange={ocr.pickModel}
              disabled={confirming}
              buttonTestId="sales-payment-vision-model-trigger"
              testId="sales-payment-vision-model-popover"
            />
            {ocr.extractionState === 'pending' && (
              <button
                type="button"
                onClick={ocr.extract}
                disabled={!ocr.selectedModel || confirming}
                title="Run OCR with the selected model"
                className="inline-flex items-center justify-center gap-1.5 rounded-lg border border-accent/40 bg-accent/10 hover:bg-accent/20 text-accent px-3 py-1.5 text-xs font-mono font-semibold transition-colors disabled:opacity-50 disabled:cursor-not-allowed disabled:bg-paper-3 disabled:border-rule-strong disabled:text-mute"
              >
                <Wand2 className="size-3.5" strokeWidth={2.5} aria-hidden />
                <span>Extract</span>
              </button>
            )}
            {ocr.extractionState === 'done' && (
              <button
                type="button"
                onClick={ocr.extract}
                disabled={!ocr.selectedModel || confirming}
                title="Re-run OCR with the selected model"
                className="inline-flex items-center justify-center gap-1.5 rounded-lg border border-rule-strong bg-paper-3 hover:bg-paper-3/80 text-ink-2 px-3 py-1.5 text-xs font-mono font-semibold transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
              >
                <Wand2 className="size-3.5" strokeWidth={2.5} aria-hidden />
                <span>Re-extract</span>
              </button>
            )}
          </div>
        )}

        {ocr.phase !== 'confirmed' && (
          <>
            <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
              <div className="sm:col-span-2 rounded-xl p-3.5 space-y-3 border-2 border-caution/30 bg-caution-soft/40">
                <SectionHeader
                  icon={<User className="size-4" strokeWidth={2.5} aria-hidden />}
                  label="Payer (customer)"
                  tone="caution"
                  trailing={<span className="text-xs font-mono normal-case tracking-normal font-semibold text-ink-2 px-2 py-0.5 rounded bg-caution-soft/60 border border-caution/30">Sent from</span>}
                />
                <div className="grid grid-cols-1 sm:grid-cols-3 gap-3">
                  <Field
                    label="Bank"
                    icon={<Banknote className="size-3" aria-hidden strokeWidth={2} />}
                    hint="optional"
                  >
                    <input
                      value={payerBankName}
                      onChange={(e) => setPayerBankName(e.target.value)}
                      disabled={disabled}
                      className={INPUT_CLS}
                      placeholder="e.g. Krungthai, SCB"
                      data-testid="sales-payment-field-payer-bank"
                    />
                  </Field>
                  <Field
                    label="Account"
                    icon={<Hash className="size-3" aria-hidden strokeWidth={2} />}
                    hint="digits"
                  >
                    <input
                      inputMode="numeric"
                      value={payerAccountNumber}
                      onChange={(e) => setPayerAccountNumber(e.target.value.replace(/[^\d]/g, ''))}
                      disabled={disabled}
                      className={`${INPUT_CLS} font-mono`}
                      placeholder="sender account"
                      data-testid="sales-payment-field-payer-account"
                    />
                  </Field>
                  <Field
                    label="Name"
                    icon={<User className="size-3" aria-hidden strokeWidth={2} />}
                    hint="optional"
                  >
                    <input
                      value={payerAccountName}
                      onChange={(e) => setPayerAccountName(e.target.value)}
                      disabled={disabled}
                      className={INPUT_CLS}
                      placeholder="sender name"
                      data-testid="sales-payment-field-payer-name"
                    />
                  </Field>
                </div>
              </div>

              <div className="sm:col-span-2 rounded-xl p-3.5 space-y-3 border-2 border-info/30 bg-info-soft/40">
                <SectionHeader
                  icon={<Building2 className="size-4" strokeWidth={2.5} aria-hidden />}
                  label="Receiver (us)"
                  tone="info"
                  trailing={<span className="text-xs font-mono normal-case tracking-normal font-semibold text-ink-2 px-2 py-0.5 rounded bg-info-soft/60 border border-info/30">Received at</span>}
                />
                <div className="grid grid-cols-1 sm:grid-cols-3 gap-3">
                  <Field
                    label="Bank"
                    icon={<Banknote className="size-3" aria-hidden strokeWidth={2} />}
                    hint="optional"
                  >
                    <div className="relative">
                      <input
                        value={receiverBankName}
                        onChange={(e) => setReceiverBankName(e.target.value)}
                        disabled={disabled}
                        className={INPUT_CLS}
                        placeholder="bank name"
                        data-testid="sales-payment-field-receiver-bank"
                      />
                      {ocr.extractionState === 'running' && !receiverBankName ? (
                        <FieldSpinner />
                      ) : ocr.extractionState === 'done' && receiverBankName.trim() ? (
                        <FilledTick filled />
                      ) : null}
                    </div>
                  </Field>
                  <Field
                    label="Account"
                    icon={<Hash className="size-3" aria-hidden strokeWidth={2} />}
                    hint="digits"
                  >
                    <div className="relative">
                      <input
                        inputMode="numeric"
                        value={receiverAccountNumber}
                        onChange={(e) => setReceiverAccountNumber(e.target.value.replace(/[^\d]/g, ''))}
                        disabled={disabled}
                        className={`${INPUT_CLS} font-mono`}
                        placeholder="receiver account"
                        data-testid="sales-payment-field-receiver-account"
                      />
                      {ocr.extractionState === 'running' && !receiverAccountNumber ? (
                        <FieldSpinner />
                      ) : ocr.extractionState === 'done' && receiverAccountNumber.trim() ? (
                        <FilledTick filled />
                      ) : null}
                    </div>
                  </Field>
                  <Field
                    label="Name"
                    icon={<User className="size-3" aria-hidden strokeWidth={2} />}
                    hint="optional"
                  >
                    <div className="relative">
                      <input
                        value={receiverAccountName}
                        onChange={(e) => setReceiverAccountName(e.target.value)}
                        disabled={disabled}
                        className={INPUT_CLS}
                        placeholder="receiver name"
                        data-testid="sales-payment-field-receiver-name"
                      />
                      {ocr.extractionState === 'running' && !receiverAccountName ? (
                        <FieldSpinner />
                      ) : ocr.extractionState === 'done' && receiverAccountName.trim() ? (
                        <FilledTick filled />
                      ) : null}
                    </div>
                  </Field>
                </div>
              </div>

              <div className="sm:col-span-2 rounded-xl p-3.5 space-y-3 border-2 border-positive/30 bg-positive-soft/40">
                <SectionHeader
                  icon={<Calendar className="size-4" strokeWidth={2.5} aria-hidden />}
                  label="Transaction"
                  tone="positive"
                />
                <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
                  <Field
                    label="Date"
                    icon={<Calendar className="size-3" aria-hidden strokeWidth={2} />}
                  >
                    <input
                      type="date"
                      value={transactionDate}
                      onChange={(e) => setTransactionDate(e.target.value)}
                      disabled={disabled}
                      className={INPUT_CLS}
                      data-testid="sales-payment-field-date"
                    />
                  </Field>
                  <Field
                    label="Amount (THB)"
                    icon={<Banknote className="size-3" aria-hidden strokeWidth={2} />}
                  >
                    <input
                      type="number"
                      step="0.01"
                      value={amount}
                      onChange={(e) => setAmount(e.target.value)}
                      disabled={disabled}
                      className={NUMBER_CLS}
                      placeholder="0.00"
                      data-testid="sales-payment-field-amount"
                    />
                  </Field>
                </div>
              </div>
            </div>

            <div
              className={`rounded-xl border p-3 space-y-1 ${
                canConfirm
                  ? 'border-positive/50 bg-positive-soft'
                  : 'border-rule-strong bg-paper-3/40'
              }`}
            >
              <div className="flex flex-wrap items-center gap-2">
                <p
                  className={`text-xs font-mono flex-1 min-w-0 ${
                    canConfirm ? 'text-positive/80' : 'text-caution/80'
                  }`}
                >
                  {confirming ? (
                    <span className="inline-flex items-center gap-1.5">
                      <Loader2 className="size-3 animate-spin" aria-hidden />
                      {t('confirm', locale)}
                    </span>
                  ) : canConfirm ? (
                    <span className="inline-flex items-center gap-1.5">
                      <CircleCheck className="size-3" strokeWidth={2.5} aria-hidden />
                      {t('ready', locale)}
                    </span>
                  ) : (
                    <span className="inline-flex items-center gap-1.5">
                      <CircleAlert className="size-3" strokeWidth={2.5} aria-hidden />
                      {t('needFields', locale)}
                    </span>
                  )}
                </p>
                <button
                  type="button"
                  onClick={handleConfirm}
                  disabled={!canConfirm || confirming}
                  title={t('confirm', locale)}
                  data-testid="sales-payment-confirm"
                  className={`w-12 h-12 inline-flex items-center justify-center gap-2 rounded-xl border text-sm font-mono font-semibold transition-colors duration-200 disabled:opacity-50 disabled:cursor-not-allowed ${
                    confirming
                      ? 'bg-rule-strong text-ink-2 border-rule-strong'
                      : canConfirm
                        ? 'bg-accent hover:bg-accent-strong text-paper-2 border-accent shadow-[0_8px_24px_-8px_color-mix(in_oklab,var(--accent)_55%,transparent)]'
                        : 'bg-paper-3 text-mute border-rule-strong'
                  }`}
                >
                  {confirming ? (
                    <Loader2 className="size-5 animate-spin" aria-hidden />
                  ) : canConfirm ? (
                    <ArrowUpRight className="size-5" aria-hidden strokeWidth={2.5} />
                  ) : (
                    <Lock className="size-5" aria-hidden strokeWidth={2} />
                  )}
                  <span className="sr-only">{t('confirm', locale)}</span>
                </button>
              </div>
            </div>

            {confirmError && (
              <p
                title={confirmError}
                className="text-xs text-critical font-mono inline-flex items-center gap-1.5"
              >
                <CircleAlert className="size-3.5" strokeWidth={2.5} aria-hidden />
                {confirmError}
              </p>
            )}

            {ocr.error && (
              <p
                title={ocr.error}
                className="text-xs text-critical font-mono inline-flex items-center gap-1.5"
              >
                <CircleAlert className="size-3.5" strokeWidth={2.5} aria-hidden />
                {ocr.error}
              </p>
            )}

            <div className="flex flex-wrap items-center justify-between gap-2 pt-2 border-t border-rule">
              {ocr.pendingFile && ocr.slipId != null && ocr.extractionState !== 'running' ? (
                <button
                  type="button"
                  onClick={ocr.removeFile}
                  disabled={confirming}
                  title="Remove · ลบ"
                  aria-label="Remove"
                  className="inline-flex items-center justify-center gap-1.5 w-9 h-9 rounded-lg border border-critical/40 bg-critical-soft text-critical hover:bg-critical/15 transition-colors disabled:opacity-50"
                >
                  <Trash2 className="size-4" strokeWidth={2} aria-hidden />
                </button>
              ) : (
                <span />
              )}
              <div className="flex items-center gap-2">
                {ocr.extractionState !== 'running' && (
                  <button
                    type="button"
                    onClick={ocr.pickAnother}
                    disabled={confirming}
                    title={ocr.pendingFile ? 'Replace file · เปลี่ยนไฟล์' : t('upload', locale)}
                    aria-label={ocr.pendingFile ? 'Replace file' : t('upload', locale)}
                    className="inline-flex items-center justify-center gap-1.5 w-9 h-9 rounded-lg border border-rule-strong bg-paper-3 hover:bg-paper-3/80 text-ink-2 transition-colors disabled:opacity-50"
                  >
                    {ocr.pendingFile ? (
                      <RefreshCw className="size-4" strokeWidth={2} aria-hidden />
                    ) : (
                      <Upload className="size-4" strokeWidth={2} aria-hidden />
                    )}
                  </button>
                )}
              </div>
            </div>
          </>
        )}

        {ocr.phase === 'confirmed' && ocr.slipId != null && (
          <div className="rounded-xl border border-positive/40 p-3 space-y-1 bg-positive-soft inline-flex items-center gap-2">
            <CircleCheck className="size-4 text-positive" strokeWidth={2.5} aria-hidden />
            <span className="text-xs font-mono text-positive inline-flex items-center gap-1">
              {t('attachedOk', locale)} · SLIP-{ocr.slipId} · SO-{soId}
            </span>
          </div>
        )}
      </div>

      {ocr.preview && (
        <Modal
          open={ocr.zoomOpen}
          onClose={() => ocr.setZoomOpen(false)}
          title="Slip preview"
          subtitle={ocr.fileName ?? undefined}
          tone="slate"
          width="2xl"
          hideCloseButton={false}
        >
          <div className="flex items-center justify-center rounded-xl p-1 border border-rule bg-paper-2">
            <img
              src={ocr.preview}
              alt="preview enlarged"
              className="max-h-[60vh] w-auto max-w-full object-contain rounded-lg"
            />
          </div>
        </Modal>
      )}
    </div>
  );
});

export default SalesPaymentSlipUpload;
