'use client';

import React, { forwardRef, useEffect, useImperativeHandle, useRef, useState } from 'react';
import Link from 'next/link';
import {
  Upload,
  UploadCloud,
  RefreshCw,
  Trash2,
  Loader2,
  CircleDot,
  Building2,
  User,
  Calendar,
  Banknote,
  ArrowUpRight,
  Lock,
  Receipt as ReceiptIcon,
  Plus,
  X,
  ZoomIn,
  CircleAlert,
  CircleCheck,
  Link as LinkIcon,
  ChevronDown,
  Eye,
  FileSpreadsheet,
  Wand2,
} from 'lucide-react';
import { Modal } from '@/components/ui/Modal';
import { submitExpenseFromSlip } from '@/app/actions/expense';
import { getSlipLockState } from '@/app/actions/slips';
import {
  FilledTick,
  Field,
  FieldSpinner,
  fileKind,
  formatBytes,
} from './SlipCard';
import { useSlipOcr } from './useSlipOcr';
import { ModelSelect } from './ModelSelect';
import type {
  BookBankFields,
  ItemRow,
  ParsedFields,
  SlipDraftFields,
  SlipUploadHandle,
  SubmitState,
} from './types';

export interface ReceiptUploadProps {
  currentUserId?: number;
  initialModels?: Parameters<typeof useSlipOcr>[0]['initialModels'];
  bookBankSlipId?: number | null;
  bookBankFields?: BookBankFields;
  onPaymentChange?: (next: 'cash' | 'credit_card' | 'transfer') => void;
  onConfirmed?: (result: {
    slipId: number;
    expenseId: number;
    status: string;
    waybillId?: string;
  }) => void;
  onSlipReady?: (slipId: number, kind: 'receipt') => void;
  onSlipDiscarded?: (slipId: number, kind: 'receipt') => void;
  onSubmitStateChange?: (state: SubmitState) => void;
  onDraftStarted?: (info: { waybillId: string; expenseId: number }) => void;
  hideSubmitButton?: boolean;
  draftWaybillId?: string | null;
}

const INPUT_CLS =
  'glass-panel w-full rounded-xl hover:bg-paper-3/60 focus:border-positive/60 focus:ring-2 focus:ring-positive/20 transition-all px-3.5 py-2.5 pr-9 text-xs text-ink focus:outline-none disabled:opacity-50 disabled:cursor-not-allowed placeholder:text-ink-2/60 font-medium';

const NUMBER_CLS =
  'glass-panel w-full rounded-xl hover:bg-paper-3/60 focus:border-positive/60 focus:ring-2 focus:ring-positive/20 transition-all px-3.5 py-2.5 pr-9 text-xs text-ink font-sans tabular-nums text-right focus:outline-none disabled:opacity-50 disabled:cursor-not-allowed placeholder:text-ink-2/60 font-medium';

const TABLE_INPUT_CLS =
  'glass-panel w-full focus:border-rule-strong rounded px-2 py-1 text-sm text-ink focus:outline-none';

export const ReceiptUpload = forwardRef<SlipUploadHandle, ReceiptUploadProps>(
  function ReceiptUpload(
    {
      currentUserId,
      initialModels,
      bookBankSlipId = null,
      bookBankFields,
      onPaymentChange,
      onConfirmed,
      onSlipReady,
      onSlipDiscarded,
      onSubmitStateChange,
      hideSubmitButton = false,
      draftWaybillId = null,
    },
    ref,
  ) {
    const ocr = useSlipOcr({
      kind: 'receipt',
      initialModels,
      currentUserId,
      onSlipReady: (id) => onSlipReady?.(id, 'receipt'),
      onSlipDiscarded: (id) => onSlipDiscarded?.(id, 'receipt'),
    });
    const [vendor, setVendor] = useState('');
    const [vendorAddress, setVendorAddress] = useState('');
    const [createdTo, setCreatedTo] = useState('');
    const [createdToAddress, setCreatedToAddress] = useState('');
    const [date, setDate] = useState('');
    const [payment, setPayment] = useState<'cash' | 'credit_card' | 'transfer'>('cash');
    const [subtotal, setSubtotal] = useState('0');
    const [vat, setVat] = useState('0');
    const [total, setTotal] = useState('0');
    const [items, setItems] = useState<ItemRow[]>([]);
    const [confirmedExpenseId, setConfirmedExpenseId] = useState<number | null>(null);
    const [confirmedStatus, setConfirmedStatus] = useState<string | null>(null);
    const [dragOver, setDragOver] = useState(false);

    function applyParsed(p: ParsedFields) {
      setVendor(p.vendorName ?? '');
      setVendorAddress(p.vendorAddress ?? '');
      setCreatedTo(p.createdTo ?? '');
      setCreatedToAddress(p.createdToAddress ?? '');
      setDate(p.transactionDate ?? '');
      setPayment((p.paymentMethod as any) ?? 'cash');
      setSubtotal(String(p.subtotal ?? 0));
      setVat(String(p.vatAmount ?? 0));
      setTotal(String(p.totalAmount ?? (Number(p.subtotal ?? 0) + Number(p.vatAmount ?? 0))));
      setItems(p.items ?? []);
    }

    useEffect(() => {
      if (ocr.parsed) applyParsed(ocr.parsed);
    }, [ocr.parsed]);

    const disabled = !ocr.pendingFile || ocr.phase === 'confirming';
    const subN = Number(subtotal);
    const vatN = Number(vat);
    const totalN = Number(total);
    const receiptMathOk =
      subN >= 0 && vatN >= 0 && totalN >= 0 && Math.abs(totalN - (subN + vatN)) <= 0.01;
    const canConfirm =
      vendor.trim().length > 0 &&
      date.length > 0 &&
      !Number.isNaN(new Date(date).getTime()) &&
      receiptMathOk;

    async function handleConfirm() {
      if (ocr.slipId == null) return;
      if (!canConfirm) {
        ocr.setError(
          payment === 'transfer' && !bookBankSlipId
            ? 'Transfer expenses require a book bank slip.'
            : 'Fill vendor + date; subtotal + VAT must equal total.',
        );
        return;
      }
      ocr.setError(null);
      ocr.setPhase('confirming');
      const overrides: any = {
        vendorName: vendor || undefined,
        vendorAddress: vendorAddress || undefined,
        createdTo: createdTo || undefined,
        createdToAddress: createdToAddress || undefined,
        transactionDate: date || undefined,
        paymentMethod: payment,
        items,
      };
      if (payment === 'transfer' && bookBankSlipId && bookBankFields) {
        overrides.bookBankSlipId = bookBankSlipId;
        overrides.bookBankFields = {
          bankName: bookBankFields.bankName || undefined,
          bankBranch: bookBankFields.bankBranch || undefined,
          accountNumber: bookBankFields.accountNumber || undefined,
          accountName: bookBankFields.accountName || undefined,
        };
      }
      const r = await submitExpenseFromSlip({
        slipId: ocr.slipId,
        actorId: currentUserId ?? 0,
        draftWaybillId: draftWaybillId ?? undefined,
        overrides,
      });
      if (!r.success || r.expenseId == null) {
        ocr.setError(r.error ?? 'Confirm failed');
        ocr.setExtractionState('done');
        ocr.setPhase('extracting');
        return;
      }
      setConfirmedExpenseId(r.expenseId);
      setConfirmedStatus(r.status ?? null);
      const lock = await getSlipLockState({ slipId: ocr.slipId, actorId: currentUserId ?? 0 });
      ocr.setLocked(lock.locked);
      ocr.setLockReason(lock.reason);
      ocr.setPhase('confirmed');
      onConfirmed?.({
        slipId: ocr.slipId,
        expenseId: r.expenseId,
        status: r.status ?? 'submission',
        waybillId: r.waybillId ?? undefined,
      });
    }

    const handleConfirmRef = useRef<() => Promise<void>>(() => Promise.resolve());
    handleConfirmRef.current = handleConfirm;
    useImperativeHandle(ref, () => ({ submit: () => handleConfirmRef.current() }), []);

    const submitState: SubmitState = {
      visible: !!ocr.pendingFile && ocr.extractionState === 'done',
      canConfirm,
      confirming: ocr.phase === 'confirming',
      pendingFile: !!ocr.pendingFile,
      isBookBank: false,
      error: ocr.error,
      hint: !canConfirm ? 'missing-fields' : 'ok',
      parsed: {
        vendorName: vendor,
        vendorAddress,
        createdTo,
        createdToAddress,
        transactionDate: date,
        paymentMethod: payment,
        subtotal: Number(subtotal) || 0,
        vatAmount: Number(vat) || 0,
        totalAmount: Number(total) || 0,
      } as SlipDraftFields,
      slipId: ocr.slipId,
    };

    const cbRef = useRef(onSubmitStateChange);
    cbRef.current = onSubmitStateChange;
    useEffect(() => {
      cbRef.current?.(submitState);
      // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [
      submitState.visible,
      submitState.canConfirm,
      submitState.confirming,
      submitState.pendingFile,
      submitState.error,
      submitState.hint,
      submitState.parsed?.vendorName,
      submitState.parsed?.transactionDate,
      submitState.parsed?.paymentMethod,
      submitState.parsed?.subtotal,
      submitState.parsed?.vatAmount,
      submitState.parsed?.totalAmount,
      submitState.slipId,
    ]);

    const confPct = Math.round(ocr.confidence * 100);
    const pendingKind = ocr.pendingFile ? fileKind(ocr.pendingFile.type, ocr.pendingFile.name) : '';

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
          <div className="flex flex-col items-stretch gap-4">
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
                className={`w-full h-40 sm:h-56 rounded-lg border-2 border-dashed flex flex-col items-center justify-center gap-1.5 cursor-pointer transition-colors duration-200 hover:border-accent/60 hover:bg-paper-3/30 ${
                  dragOver
                    ? 'border-positive'
                    : 'border-rule-strong bg-paper-3/40 hover:border-positive/50'
                }`}
                data-testid="slip-drop-zone"
                aria-label="Drop a slip or click to browse"
              >
                <UploadCloud className="size-9 text-accent" strokeWidth={1.5} aria-hidden />
                <p className="text-xs font-sans tabular-nums text-ink-2 text-center px-3">
                  Drop or click
                </p>
                <p className="text-xs font-sans tabular-nums text-mute text-center px-3 uppercase tracking-wider">
                  JPG · PNG · WEBP · PDF · ≤ 20 MB
                </p>
              </div>
            ) : ocr.preview ? (
              <button
                type="button"
                onClick={() => ocr.setZoomOpen(true)}
                disabled={ocr.extractionState === 'running'}
                className="relative w-full group rounded-lg hover:border-positive/50 transition-colors duration-200 disabled:cursor-default disabled:hover:border-rule border border-rule bg-paper-3 overflow-hidden ring-1 ring-rule/50"
                title={ocr.extractionState === 'running' ? '' : 'Click to enlarge'}
                data-testid="slip-preview-zoom"
              >
                <img
                  src={ocr.preview}
                  alt="preview"
                  className="w-full max-h-72 sm:max-h-80 object-contain rounded-lg"
                />
                {ocr.extractionState !== 'running' && (
                  <span
                    aria-hidden
                    className="absolute bottom-1.5 right-1.5 grid place-items-center w-6 h-6 rounded-md ring-1 ring-rule text-sm text-ink opacity-70 group-hover:opacity-100 transition-opacity bg-paper-2"
                  >
                    <ZoomIn className="size-3.5" strokeWidth={2} />
                  </span>
                )}
              </button>
            ) : (
              <div className="w-full h-32 sm:h-44 rounded-xl flex items-center justify-center border border-rule bg-paper-3">
                <FileSpreadsheet className="size-12 text-ink-2" strokeWidth={1.2} aria-hidden />
              </div>
            )}

            {ocr.pendingFile && (
              <div className="w-full min-w-0 space-y-2">
                <div className="flex items-center gap-2 flex-wrap">
                  <span
                    className={`px-2.5 py-1 rounded-md text-xs font-sans tabular-nums uppercase font-bold inline-flex items-center gap-1 ${
                      ocr.phase === 'confirmed'
                        ? 'bg-positive-soft text-positive border border-positive/40'
                        : ocr.extractionState === 'pending'
                          ? 'bg-rule-strong/40 text-ink-2'
                          : ocr.extractionState === 'running'
                            ? 'bg-caution-soft text-caution border border-caution/40'
                            : 'bg-positive-soft text-positive border border-positive/40'
                    }`}
                    title={
                      ocr.phase === 'confirmed'
                        ? 'Confirmed'
                        : ocr.extractionState === 'pending'
                          ? 'Ready to extract'
                          : ocr.extractionState === 'running'
                            ? `Extracting · ${ocr.elapsed}s`
                            : 'Review OCR'
                    }
                    data-testid="slip-step-badge"
                  >
                    {ocr.phase === 'confirmed' ? (
                      <CircleCheck className="size-3.5" strokeWidth={2.5} />
                    ) : ocr.extractionState === 'running' ? (
                      <Loader2 className="size-3.5 animate-spin" strokeWidth={2.5} />
                    ) : ocr.extractionState === 'done' && canConfirm ? (
                      <CircleCheck className="size-3.5" strokeWidth={2.5} />
                    ) : (
                      <span className="font-sans tabular-nums">2/2</span>
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
                  <span className="text-xs font-sans tabular-nums text-ink-2 inline-flex items-center gap-1">
                    <FileSpreadsheet className="size-3" aria-hidden strokeWidth={2} />
                    {pendingKind} · {formatBytes(ocr.pendingFile.size)}
                  </span>
                  {ocr.extractionState === 'done' && (
                    <span
                      title={`${confPct}% confidence · mode ${ocr.mode}`}
                      className="text-xs font-sans tabular-nums text-ink-2"
                    >
                      <CircleDot className="size-3 inline-block mr-0.5 text-positive" strokeWidth={2.5} />
                      {confPct}%
                    </span>
                  )}
                </div>
                <p
                  className="text-sm font-bold text-ink truncate"
                  title={ocr.pendingFile.name}
                >
                  {ocr.pendingFile.name}
                </p>

                {ocr.extractionState === 'running' && (
                  <div className="space-y-2">
                    <p className="text-xs text-ink-2 font-sans tabular-nums inline-flex items-center gap-1.5">
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

                {ocr.phase === 'confirmed' && confirmedStatus && (
                  <p
                    title={`Submitted → ${confirmedStatus}`}
                    className="text-xs text-ink-2 font-sans tabular-nums inline-flex items-center gap-1"
                  >
                    <CircleCheck className="size-3 text-positive" strokeWidth={2.5} />
                    Submitted
                  </p>
                )}

                {ocr.pendingFile && ocr.extractionState !== 'running' && ocr.visionModels.length > 0 && (
                  <div className="flex flex-wrap items-center gap-2 pt-2">
                    <div className="inline-flex items-center gap-2 pl-1.5 pr-1 py-1 rounded-lg border border-rule bg-paper-3/40">
                      <span className="text-[10px] font-sans tabular-nums uppercase tracking-widest text-mute shrink-0 inline-flex items-center gap-1">
                        <Eye className="size-3" strokeWidth={2} aria-hidden />
                        Model
                      </span>
                      <ModelSelect
                        models={ocr.visionModels}
                        value={ocr.selectedModel}
                        onChange={ocr.pickModel}
                        disabled={ocr.phase === 'confirming'}
                        testId="slip-vision-model-picker"
                        buttonTestId="slip-vision-model-trigger"
                      />
                    </div>
                    {ocr.extractionState === 'pending' && (
                      <button
                        type="button"
                        onClick={ocr.extract}
                        disabled={!ocr.selectedModel || ocr.phase === 'confirming'}
                        title="Run OCR with the selected model"
                        data-testid="slip-extract"
                        className="inline-flex items-center justify-center gap-1.5 rounded-lg border border-accent/40 bg-accent/10 hover:bg-accent/20 text-accent px-3 py-1.5 text-xs font-sans tabular-nums font-semibold transition-colors disabled:opacity-50 disabled:cursor-not-allowed disabled:bg-paper-3 disabled:border-rule-strong disabled:text-mute"
                      >
                        <Wand2 className="size-3.5" strokeWidth={2.5} aria-hidden />
                        <span>Extract</span>
                      </button>
                    )}
                  </div>
                )}
              </div>
            )}
          </div>

          {ocr.phase !== 'confirmed' && ocr.extractionState !== 'pending' && (
            <>
              <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
                <div className="sm:col-span-2 pt-4 pb-1 space-y-3 border-t border-rule first:border-t-0 first:pt-0">
                  <div className="flex items-center gap-2 pb-2">
                    <span aria-hidden className="text-info">{<Building2 className="size-4" strokeWidth={2.5} />}</span>
                    <h4 className="text-sm font-semibold text-ink">Vendor</h4>
                    <span className="text-xs font-sans tabular-nums text-mute normal-case tracking-normal">Created from</span>
                  </div>
                  <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
                    <Field label="Name" icon={<Building2 className="size-3" aria-hidden strokeWidth={2} />}>
                      <div className="relative">
                        <input
                          value={vendor}
                          onChange={(e) => setVendor(e.target.value)}
                          disabled={disabled}
                          className={INPUT_CLS}
                          placeholder="Vendor Name"
                          data-testid="slip-field-vendor"
                        />
                        {ocr.extractionState === 'running' && !vendor ? (
                          <FieldSpinner />
                        ) : ocr.extractionState === 'done' && vendor.trim() ? (
                          <FilledTick filled />
                        ) : null}
                      </div>
                    </Field>
                    <Field label="Address" icon={<Building2 className="size-3" aria-hidden strokeWidth={2} />}>
                      <div className="relative">
                        <input
                          value={vendorAddress}
                          onChange={(e) => setVendorAddress(e.target.value)}
                          disabled={disabled}
                          className={INPUT_CLS}
                          placeholder="Vendor Address"
                          data-testid="slip-field-vendor-address"
                        />
                        {ocr.extractionState === 'running' && !vendorAddress ? (
                          <FieldSpinner />
                        ) : ocr.extractionState === 'done' && vendorAddress.trim() ? (
                          <FilledTick filled />
                        ) : null}
                      </div>
                    </Field>
                  </div>
                </div>

                <div className="sm:col-span-2 pt-4 pb-1 space-y-3 border-t border-rule first:border-t-0 first:pt-0">
                  <div className="flex items-center gap-2 pb-2">
                    <span aria-hidden className="text-info">{<User className="size-4" strokeWidth={2.5} />}</span>
                    <h4 className="text-sm font-semibold text-ink">Customer</h4>
                    <span className="text-xs font-sans tabular-nums text-mute normal-case tracking-normal">Created to</span>
                  </div>
                  <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
                    <Field label="Name" icon={<User className="size-3" aria-hidden strokeWidth={2} />}>
                      <div className="relative">
                        <input
                          value={createdTo}
                          onChange={(e) => setCreatedTo(e.target.value)}
                          disabled={disabled}
                          className={INPUT_CLS}
                          placeholder="Customer Name"
                          data-testid="slip-field-created-to"
                        />
                        {ocr.extractionState === 'running' && !createdTo ? (
                          <FieldSpinner />
                        ) : ocr.extractionState === 'done' && createdTo.trim() ? (
                          <FilledTick filled />
                        ) : null}
                      </div>
                    </Field>
                    <Field label="Address" icon={<User className="size-3" aria-hidden strokeWidth={2} />}>
                      <div className="relative">
                        <input
                          value={createdToAddress}
                          onChange={(e) => setCreatedToAddress(e.target.value)}
                          disabled={disabled}
                          className={INPUT_CLS}
                          placeholder="Customer Address"
                          data-testid="slip-field-created-to-address"
                        />
                        {ocr.extractionState === 'running' && !createdToAddress ? (
                          <FieldSpinner />
                        ) : ocr.extractionState === 'done' && createdToAddress.trim() ? (
                          <FilledTick filled />
                        ) : null}
                      </div>
                    </Field>
                  </div>
                </div>

                <div className="sm:col-span-2 pt-4 pb-1 space-y-3 border-t border-rule first:border-t-0 first:pt-0">
                  <div className="flex items-center gap-2 pb-2">
                    <span aria-hidden className="text-caution">{<Calendar className="size-4" strokeWidth={2.5} />}</span>
                    <h4 className="text-sm font-semibold text-ink">Transaction</h4>
                  </div>
                  <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
                    <Field label="Date" icon={<Calendar className="size-3" aria-hidden strokeWidth={2} />}>
                      <div className="relative">
                        <input
                          type="date"
                          value={date}
                          onChange={(e) => setDate(e.target.value)}
                          disabled={disabled}
                          className={INPUT_CLS}
                          data-testid="slip-field-date"
                        />
                        {ocr.extractionState === 'running' && !date ? (
                          <FieldSpinner />
                        ) : ocr.extractionState === 'done' && date ? (
                          <FilledTick filled />
                        ) : null}
                      </div>
                    </Field>
                    <Field label="Pay" icon={<Banknote className="size-3" aria-hidden strokeWidth={2} />}>
                      <div className="relative">
                        <select
                          value={payment}
                          onChange={(e) => {
                            const v = e.target.value as typeof payment;
                            setPayment(v);
                            onPaymentChange?.(v);
                          }}
                          disabled={disabled || ocr.extractionState === 'running'}
                          className="w-full rounded-xl hover:bg-paper-3/60 focus:border-positive/60 focus:ring-2 focus:ring-positive/20 transition-all px-3.5 py-2.5 pr-9 text-xs text-ink focus:outline-none disabled:opacity-50 disabled:cursor-not-allowed font-medium border border-rule bg-paper-2"
                          title="Payment method"
                        >
                          <option value="cash">Cash</option>
                          <option value="credit_card">Credit card</option>
                          <option value="transfer">Transfer</option>
                        </select>
                        {ocr.extractionState === 'running' && <FieldSpinner />}
                      </div>
                    </Field>
                  </div>
                </div>

                <div className="sm:col-span-2 pt-4 pb-1 space-y-3 border-t border-rule first:border-t-0 first:pt-0">
                  <div className="flex items-center gap-2 pb-2">
                    <span aria-hidden className="text-positive">{<Banknote className="size-4" strokeWidth={2.5} />}</span>
                    <h4 className="text-sm font-semibold text-ink">Summary</h4>
                  </div>
                  <div className="grid grid-cols-1 sm:grid-cols-3 gap-3">
                    {([
                      { key: 'Subtotal', value: subtotal, set: setSubtotal },
                      { key: 'VAT', value: vat, set: setVat },
                      { key: 'Total', value: total, set: setTotal },
                    ] as const).map(({ key, value, set }) => (
                      <Field
                        key={key}
                        label={key}
                        icon={<Banknote className="size-3" aria-hidden strokeWidth={2} />}
                      >
                        <div className="relative">
                          <input
                            type="number"
                            step="0.01"
                            value={value}
                            onChange={(e) => set(e.target.value)}
                            disabled={disabled}
                            className={NUMBER_CLS}
                            data-testid={`slip-field-${key.toLowerCase()}`}
                          />
                          {ocr.extractionState === 'running' && value === '0' ? (
                            <FieldSpinner />
                          ) : ocr.extractionState === 'done' && (Number(value) > 0 || value !== '0') ? (
                            <FilledTick filled />
                          ) : null}
                        </div>
                      </Field>
                    ))}
                  </div>
                </div>
              </div>

              {ocr.pendingFile && ocr.extractionState === 'done' && !hideSubmitButton && (
                <div
                  className={`rounded-xl border p-3 space-y-1 ${
                    canConfirm
                      ? 'border-positive/50 bg-positive-soft'
                      : 'border-rule-strong bg-paper-3/40'
                  }`}
                >
                  <div className="flex flex-wrap items-center gap-2">
                    <p
                      className={`text-xs font-sans tabular-nums flex-1 min-w-0 ${
                        canConfirm ? 'text-positive/80' : 'text-caution/80'
                      }`}
                    >
                      {ocr.phase === 'confirming' ? (
                        <span className="inline-flex items-center gap-1.5">
                          <Loader2 className="size-3 animate-spin" aria-hidden />
                          Saving as draft
                        </span>
                      ) : canConfirm ? (
                        <span className="inline-flex items-center gap-1.5">
                          <CircleCheck className="size-3" strokeWidth={2.5} aria-hidden />
                          All required fields look good
                        </span>
                      ) : payment === 'transfer' && !bookBankSlipId ? (
                        <span className="inline-flex items-center gap-1.5">
                          <CircleAlert className="size-3" strokeWidth={2.5} aria-hidden />
                          Transfer requires a book bank slip
                        </span>
                      ) : (
                        <span className="inline-flex items-center gap-1.5">
                          <CircleAlert className="size-3" strokeWidth={2.5} aria-hidden />
                          Fill vendor + date; subtotal + VAT must equal total
                        </span>
                      )}
                    </p>
                    <button
                      type="button"
                      onClick={handleConfirm}
                      disabled={ocr.phase === 'confirming' || !canConfirm}
                      title={
                        canConfirm
                          ? 'Send & confirm · ส่งและยืนยัน'
                          : 'Disabled · ปิดอยู่'
                      }
                      data-testid="slip-confirm"
                      className={`w-12 h-12 inline-flex items-center justify-center gap-2 rounded-xl border text-sm font-sans tabular-nums font-semibold transition-colors duration-200 disabled:opacity-50 disabled:cursor-not-allowed ${
                        ocr.phase === 'confirming'
                          ? 'bg-rule-strong text-ink-2 border-rule-strong'
                          : canConfirm
                            ? 'bg-accent hover:bg-accent-strong text-paper-2 border-accent shadow-[0_8px_24px_-8px_color-mix(in_oklab,var(--accent)_55%,transparent)]'
                            : 'bg-paper-3 text-mute border-rule-strong'
                      }`}
                    >
                      {ocr.phase === 'confirming' ? (
                        <Loader2 className="size-5 animate-spin" aria-hidden />
                      ) : canConfirm ? (
                        <ArrowUpRight className="size-5" aria-hidden strokeWidth={2.5} />
                      ) : (
                        <Lock className="size-5" aria-hidden strokeWidth={2} />
                      )}
                      <span className="sr-only">
                        {canConfirm ? 'Send & confirm' : 'Send & confirm (disabled)'}
                      </span>
                    </button>
                  </div>
                </div>
              )}

              {ocr.pendingFile && ocr.extractionState === 'done' && (
                <div className="rounded-xl overflow-hidden border border-rule bg-paper-3/40">
                  <details className="group" open>
                    <summary
                      className="flex items-center justify-between px-3 py-2 border-b border-rule cursor-pointer select-none hover:bg-paper-3/60 transition-colors [&::-webkit-details-marker]:hidden"
                      title="OCR line items"
                    >
                      <span className="text-xs font-sans tabular-nums text-ink-2 uppercase tracking-widest font-semibold inline-flex items-center gap-1.5">
                        <FileSpreadsheet className="size-3.5 text-positive" strokeWidth={2} aria-hidden />
                        Items
                        <span className="text-mute normal-case tracking-normal">({items.length})</span>
                      </span>
                      <ChevronDown className="size-4 text-mute transition-transform duration-200 group-open:rotate-180" />
                    </summary>
                    <div className="p-3 space-y-3">
                      <div className="overflow-x-auto">
                        <table className="w-full text-left border-collapse text-sm font-sans tabular-nums">
                          <thead>
                            <tr className="border-b border-rule text-mute uppercase tracking-wider text-xs">
                              <th title="Description" className="py-1.5 px-2 pb-2 text-ink-2 font-semibold">
                                <FileSpreadsheet className="size-3 inline-block mr-1" aria-hidden strokeWidth={2} />
                                Desc
                              </th>
                              <th title="Quantity" className="py-1.5 px-2 pb-2 text-center w-20 text-ink-2 font-semibold">Qty</th>
                              <th title="Unit Price" className="py-1.5 px-2 pb-2 text-right w-24 text-ink-2 font-semibold">Unit</th>
                              <th title="Amount" className="py-1.5 px-2 pb-2 text-right w-28 text-ink-2 font-semibold">THB</th>
                              <th className="py-1.5 px-2 pb-2 text-center w-10"></th>
                            </tr>
                          </thead>
                          <tbody className="divide-y divide-rule">
                            {items.map((it, i) => (
                              <tr key={i} className="hover:bg-paper-3/10 transition-colors group/row">
                                <td className="py-2 px-1">
                                  <input
                                    type="text"
                                    value={it.description}
                                    onChange={(e) =>
                                      setItems((arr) =>
                                        arr.map((x, idx) =>
                                          idx === i ? { ...x, description: e.target.value } : x,
                                        ),
                                      )
                                    }
                                    disabled={ocr.phase === 'confirming'}
                                    placeholder="Description"
                                    className={TABLE_INPUT_CLS}
                                  />
                                </td>
                                <td className="py-2 px-1 text-center">
                                  <input
                                    type="number"
                                    step="any"
                                    value={it.qty ?? 1}
                                    onChange={(e) =>
                                      setItems((arr) =>
                                        arr.map((x, idx) => {
                                          if (idx !== i) return x;
                                          const qty = Number(e.target.value) || 0;
                                          const up = x.unitPrice ?? x.amount ?? 0;
                                          return { ...x, qty, unitPrice: up, amount: qty * up };
                                        }),
                                      )
                                    }
                                    disabled={ocr.phase === 'confirming'}
                                    className="w-16 focus:border-rule-strong rounded px-1.5 py-1 text-center text-sm font-sans tabular-nums text-ink focus:outline-none border border-rule bg-paper-2"
                                  />
                                </td>
                                <td className="py-2 px-1 text-right">
                                  <input
                                    type="number"
                                    step="0.01"
                                    value={it.unitPrice ?? it.amount ?? 0}
                                    onChange={(e) =>
                                      setItems((arr) =>
                                        arr.map((x, idx) => {
                                          if (idx !== i) return x;
                                          const unitPrice = Number(e.target.value) || 0;
                                          const q = x.qty ?? 1;
                                          return { ...x, unitPrice, amount: q * unitPrice };
                                        }),
                                      )
                                    }
                                    disabled={ocr.phase === 'confirming'}
                                    className="w-20 focus:border-rule-strong rounded px-1.5 py-1 text-right text-sm font-sans tabular-nums text-ink focus:outline-none border border-rule bg-paper-2"
                                  />
                                </td>
                                <td className="py-2 px-1 text-right">
                                  <input
                                    type="number"
                                    step="0.01"
                                    value={it.amount}
                                    onChange={(e) =>
                                      setItems((arr) =>
                                        arr.map((x, idx) =>
                                          idx === i
                                            ? { ...x, amount: Number(e.target.value) || 0 }
                                            : x,
                                        ),
                                      )
                                    }
                                    disabled={ocr.phase === 'confirming'}
                                    className="w-24 focus:border-rule-strong rounded px-1.5 py-1 text-right text-sm font-sans tabular-nums font-semibold text-positive focus:outline-none border border-rule bg-paper-2"
                                  />
                                </td>
                                <td className="py-2 px-1 text-center">
                                  <button
                                    type="button"
                                    onClick={() => setItems((arr) => arr.filter((_, idx) => idx !== i))}
                                    disabled={ocr.phase === 'confirming'}
                                    className="text-mute hover:text-critical transition-colors p-1"
                                    title="Remove item"
                                    aria-label={`Remove item ${i + 1}`}
                                  >
                                    <X className="size-3.5" strokeWidth={2} aria-hidden />
                                  </button>
                                </td>
                              </tr>
                            ))}
                          </tbody>
                        </table>
                      </div>
                      <div className="flex justify-end pt-1">
                        <button
                          type="button"
                          onClick={() =>
                            setItems((arr) => [
                              ...arr,
                              { description: '', qty: 1, unitPrice: 0, amount: 0 },
                            ])
                          }
                          disabled={ocr.phase === 'confirming'}
                          className="px-2.5 py-1 rounded hover:bg-rule-strong border border-rule-strong text-ink text-xs font-sans tabular-nums transition-colors inline-flex items-center gap-1"
                        >
                          <Plus className="size-3" strokeWidth={2.5} aria-hidden />
                          Add
                        </button>
                      </div>
                    </div>
                  </details>
                </div>
              )}

              {ocr.error && (
                <p
                  title={ocr.error}
                  className="text-xs text-critical font-sans tabular-nums inline-flex items-center gap-1.5"
                >
                  <CircleAlert className="size-3.5" strokeWidth={2.5} aria-hidden />
                  {ocr.error}
                </p>
              )}

              {ocr.pendingFile && ocr.extractionState === 'done' && ocr.selectedModelDesc && (
                <p
                  title={ocr.selectedModelDesc}
                  className="text-xs text-mute italic line-clamp-2"
                >
                  {ocr.selectedModelDesc}
                </p>
              )}

              <div className="flex flex-wrap items-center justify-between gap-2 pt-3 border-t border-rule-strong">
                {ocr.pendingFile && ocr.slipId != null && ocr.extractionState !== 'running' ? (
                  <button
                    type="button"
                    onClick={ocr.removeFile}
                    disabled={ocr.phase === 'confirming'}
                    title="Remove · ลบ"
                    aria-label="Remove"
                    data-testid="slip-remove"
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
                      disabled={ocr.phase === 'confirming'}
                      title={ocr.pendingFile ? 'Replace file · เปลี่ยนไฟล์' : 'Pick a file · เลือกไฟล์'}
                      aria-label={ocr.pendingFile ? 'Replace file' : 'Pick a file'}
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
            <>
              <div className="flex items-start gap-4">
                {ocr.preview ? (
                  <button
                    type="button"
                    onClick={() => ocr.setZoomOpen(true)}
                    className="shrink-0 rounded-lg hover:border-positive/50 transition-colors cursor-zoom-in overflow-hidden border border-rule bg-paper-3"
                    title="Click to enlarge"
                  >
                    <img src={ocr.preview} alt="preview" className="w-20 h-20 object-cover" />
                  </button>
                ) : (
                  <div className="w-20 h-20 shrink-0 rounded-lg flex items-center justify-center text-2xl border border-rule bg-paper-3">
                    <ReceiptIcon className="size-8 text-ink-2" strokeWidth={1.5} aria-hidden />
                  </div>
                )}
                <div className="flex-1 min-w-0">
                  <p className="text-sm font-bold text-ink truncate">{vendor || ocr.fileName}</p>
                  <p className="text-xs text-ink-2 font-sans tabular-nums">
                    {confirmedExpenseId != null && `EXP-${confirmedExpenseId} · `}
                    SLIP-{ocr.slipId}
                  </p>
                </div>
                <Link
                  href={
                    confirmedExpenseId != null
                      ? `/waybill/by-expense/${confirmedExpenseId}`
                      : '/inbox?scope=waiting'
                  }
                  title="Open in expense workflow"
                  aria-label="Open in expense workflow"
                  className="text-info-strong hover:text-info inline-flex items-center justify-center gap-1.5 w-9 h-9 rounded-lg border border-info/30 glass-tint-info hover:shadow-popover transition-colors"
                >
                  <LinkIcon className="size-4" strokeWidth={2} aria-hidden />
                </Link>
              </div>

              {!ocr.locked && (
                <div className="flex items-center justify-end gap-2 pt-2 border-t border-rule">
                  <button
                    type="button"
                    onClick={ocr.removeFile}
                    title="Remove · ลบ"
                    aria-label="Remove"
                    className="inline-flex items-center justify-center w-9 h-9 rounded-lg border border-critical/40 bg-critical-soft text-critical hover:bg-critical/15 transition-colors"
                    data-testid="slip-remove-confirmed"
                  >
                    <Trash2 className="size-4" strokeWidth={2} aria-hidden />
                  </button>
                </div>
              )}
              {ocr.locked && ocr.lockReason && (
                <p
                  title={ocr.lockReason}
                  className="text-xs font-sans tabular-nums text-mute border-t border-rule pt-2 inline-flex items-center gap-1.5"
                >
                  <Lock className="size-3" strokeWidth={2} aria-hidden />
                  {ocr.lockReason}
                </p>
              )}

              <div className="flex items-center justify-end pt-1">
                <button
                  type="button"
                  onClick={ocr.pickAnother}
                  title="Upload another slip · อัพโหลดอีกใบ"
                  aria-label="Upload another slip"
                  className="inline-flex items-center justify-center w-9 h-9 rounded-lg border border-rule-strong bg-paper-3 text-ink-2 hover:bg-paper-3/80 transition-colors"
                >
                  <Plus className="size-4" strokeWidth={2.5} aria-hidden />
                </button>
              </div>
            </>
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
            <div className="glass-panel flex items-center justify-center rounded-xl p-1">
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
  },
);

export default ReceiptUpload;