'use client';

import React, { forwardRef, useEffect, useImperativeHandle, useRef, useState } from 'react';
import Link from 'next/link';
import { Modal } from '@/components/ui/Modal';
import { submitExpenseFromSlip, getSlipLockState } from '@/app/actions';
import {
  FilledTick,
  Field,
  FieldSpinner,
  ModelCard,
  SectionHeader,
  fileKind,
  formatBytes,
} from './SlipCard';
import { useSlipOcr } from './useSlipOcr';
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
  autoExtract?: boolean;
  draftWaybillId?: string | null;
}

const INPUT_CLS =
  'glass-panel w-full rounded-xl hover:bg-paper-3/60 focus:border-positive/60 focus:ring-2 focus:ring-positive/20 transition-all px-3.5 py-2.5 pr-9 text-xs text-white focus:outline-none disabled:opacity-50 disabled:cursor-not-allowed placeholder-mute font-medium';

const NUMBER_CLS =
  'glass-panel w-full rounded-xl hover:bg-paper-3/60 focus:border-positive/60 focus:ring-2 focus:ring-positive/20 transition-all px-3.5 py-2.5 pr-9 text-xs text-white font-mono text-right focus:outline-none disabled:opacity-50 disabled:cursor-not-allowed placeholder-mute font-medium';

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
      autoExtract = true,
      draftWaybillId = null,
    },
    ref,
  ) {
    const ocr = useSlipOcr({
      kind: 'receipt',
      initialModels,
      currentUserId,
      autoExtract,
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
        <h3 className="text-xs font-bold text-ink uppercase tracking-wider flex items-center gap-2 font-mono">
          <span>📤</span>
          {ocr.phase === 'confirmed'
            ? 'Slip Upload (Confirmed)'
            : 'Slip Upload (Upload → OCR → Review → Confirm)'}
        </h3>

        <input
          ref={ocr.inputRef}
          type="file"
          accept="image/*,application/pdf"
          onChange={ocr.onPick}
          className="hidden"
        />

        <div className="glass-panel rounded-2xl p-5 space-y-4">
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
                className={`glass-tint-positive shrink-0 w-48 h-64 rounded-xl border-2 border-dashed flex flex-col items-center justify-center gap-2 cursor-pointer transition-colors ${
                  dragOver
                    ? 'border-positive '
                    : 'border-rule-strong bg-paper-3/40 hover:border-positive/50'
                }`}
                data-testid="slip-drop-zone"
              >
                <span className="text-4xl">📄</span>
                <p className="text-xs font-mono text-ink-2 text-center px-3">
                  Drag &amp; drop a slip
                  <br />
                  or click to browse
                </p>
              </div>
            ) : ocr.preview ? (
              <button
                type="button"
                onClick={() => ocr.setZoomOpen(true)}
                disabled={ocr.extractionState === 'running'}
                className="glass-panel relative shrink-0 group rounded-xl hover:border-positive/50 transition-colors disabled:cursor-default disabled:hover:border-rule"
                title={ocr.extractionState === 'running' ? '' : 'Click to enlarge'}
                data-testid="slip-preview-zoom"
              >
                <img
                  src={ocr.preview}
                  alt="preview"
                  className={`glass-panel object-contain rounded-xl ${
                    ocr.extractionState === 'running' ? 'w-32 h-40' : 'w-48 h-64'
                  }`}
                />
                {ocr.extractionState !== 'running' && (
                  <span
                    aria-hidden
                    className="glass-panel absolute bottom-1.5 right-1.5 grid place-items-center w-6 h-6 rounded-full ring-1 ring-rule-strong text-sm text-ink opacity-0 group-hover:opacity-100 transition-opacity"
                  >
                    🔍
                  </span>
                )}
              </button>
            ) : (
              <div
                className={`glass-panel shrink-0 rounded-xl flex items-center justify-center text-5xl ${
                  ocr.extractionState === 'running' ? 'w-32 h-40' : 'w-48 h-64'
                }`}
              >
                📄
              </div>
            )}

            <div className="flex-1 min-w-0 space-y-2">
              <div className="flex items-center gap-2 flex-wrap">
                <span
                  className={`glass-tint-caution px-2 py-0.5 rounded-full text-xs font-mono uppercase ${
                    ocr.phase === 'confirmed'
                      ? 'bg-positive-soft text-positive'
                      : !ocr.pendingFile
                        ? 'bg-rule-strong/40 text-ink-2'
                        : ocr.extractionState === 'pending'
                          ? 'bg-rule-strong/40 text-ink-2'
                          : ocr.extractionState === 'running'
                            ? ' text-caution'
                            : 'bg-positive-soft text-positive'
                  }`}
                  data-testid="slip-step-badge"
                >
                  {ocr.phase === 'confirmed'
                    ? 'Step 3 · Confirmed'
                    : !ocr.pendingFile
                      ? 'Step 1 · Pick model & file'
                      : ocr.extractionState === 'pending'
                        ? 'Step 2 · Ready to extract'
                        : ocr.extractionState === 'running'
                          ? `Step 2 · Extracting · ${ocr.elapsed}s`
                          : canConfirm
                            ? 'Step 2 · Review OCR ✓ filled'
                            : 'Step 2 · Review OCR'}
                </span>
                {ocr.pendingFile && (
                  <span className="text-xs font-mono text-ink-2">
                    {pendingKind} · {formatBytes(ocr.pendingFile.size)}
                  </span>
                )}
                {ocr.pendingFile && ocr.extractionState === 'done' && (
                  <span className="text-xs font-mono text-ink-2">
                    {confPct}% confidence · {ocr.mode}
                  </span>
                )}
              </div>
              <p className="text-sm font-bold text-white truncate">
                {ocr.pendingFile ? ocr.pendingFile.name : 'No file selected'}
              </p>

              {ocr.phase !== 'confirmed' &&
                ocr.extractionState !== 'running' &&
                ocr.extractionState !== 'done' &&
                ocr.visionModels.length > 0 && (
                  <div className="space-y-2 pt-1">
                    <span className="text-xs font-mono text-ink-2 uppercase tracking-wider">
                      Vision model
                    </span>
                    <div
                      className="grid grid-cols-1 md:grid-cols-2 gap-2"
                      data-testid="slip-vision-model"
                    >
                      {ocr.visionModels.map((m) => (
                        <div key={m.id} className="min-w-0">
                          <ModelCard
                            m={m}
                            selected={m.name === ocr.selectedModel}
                            onSelect={ocr.pickModel}
                            testId={`slip-vision-model-${m.id}`}
                            disabled={ocr.phase === 'confirming'}
                          />
                        </div>
                      ))}
                    </div>
                  </div>
                )}
              {ocr.phase !== 'confirmed' &&
                ocr.extractionState !== 'running' &&
                ocr.extractionState !== 'done' &&
                ocr.visionModels.length === 0 && (
                  <p className="text-xs text-mute italic">Loading models…</p>
                )}

              {ocr.extractionState === 'running' && (
                <div className="space-y-2">
                  <p className="text-xs text-ink-2 font-mono">
                    Running vision model <span className="text-ink">{ocr.selectedModel || '…'}</span>
                    {ocr.elapsed >= 5 && (
                      <span className="text-mute"> · large models can take 1-3 min</span>
                    )}
                  </p>
                  <div className="glass-panel relative w-full h-1.5 rounded-full overflow-hidden">
                    <div
                      className="absolute inset-y-0 left-0 bg-positive/40 w-1/3 rounded-full"
                      style={{ animation: 'slip-indeterminate 1.4s ease-in-out infinite' }}
                    />
                  </div>
                </div>
              )}

              {ocr.phase === 'confirmed' && confirmedStatus && (
                <p className="text-xs text-ink-2 font-mono">
                  Submitted → {confirmedStatus}
                </p>
              )}
            </div>
          </div>

          {!ocr.pendingFile && ocr.phase !== 'confirmed' && (
            <div className="glass-panel rounded-xl border border-rule-strong p-4">
              <p className="text-sm font-mono text-center text-ink-2 py-3">
                Drop a slip above or click to pick a file · Supports JPG / PNG / WEBP / PDF · Max 20 MB
              </p>
            </div>
          )}

          {ocr.phase !== 'confirmed' && (
            <>
              <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
                <div className="glass-panel sm:col-span-2 rounded-xl p-3.5 space-y-3">
                  <SectionHeader icon="🏢" label="Created from (Vendor)" tone="info" />
                  <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
                    <Field label="Vendor Name">
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
                    <Field label="Vendor Address">
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

                <div className="glass-panel sm:col-span-2 rounded-xl p-3.5 space-y-3">
                  <SectionHeader icon="👤" label="Created to (Customer)" tone="info" />
                  <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
                    <Field label="Customer Name">
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
                    <Field label="Customer Address">
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

                <div className="glass-panel sm:col-span-2 rounded-xl p-3.5 space-y-3">
                  <SectionHeader icon="📅" label="Transaction Details" tone="caution" />
                  <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
                    <Field label="Transaction date">
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
                    <Field label="Payment">
                      <div className="relative">
                        <select
                          value={payment}
                          onChange={(e) => {
                            const v = e.target.value as typeof payment;
                            setPayment(v);
                            onPaymentChange?.(v);
                          }}
                          disabled={disabled || ocr.extractionState === 'running'}
                          className="glass-panel w-full rounded-xl hover:bg-paper-3/60 focus:border-positive/60 focus:ring-2 focus:ring-positive/20 transition-all px-3.5 py-2.5 pr-9 text-xs text-white focus:outline-none disabled:opacity-50 disabled:cursor-not-allowed font-medium"
                        >
                          <option value="cash" className="glass-panel">Cash</option>
                          <option value="credit_card" className="glass-panel">Credit card</option>
                          <option value="transfer" className="glass-panel">Transfer</option>
                        </select>
                        {ocr.extractionState === 'running' && <FieldSpinner />}
                      </div>
                    </Field>
                  </div>
                </div>

                <div className="glass-panel sm:col-span-2 rounded-xl p-3.5 space-y-3">
                  <SectionHeader icon="💵" label="Financial Summary" tone="positive" />
                  <div className="grid grid-cols-1 sm:grid-cols-3 gap-3">
                    {(['Subtotal', 'VAT', 'Total'] as const).map((lbl) => {
                      const value = lbl === 'Subtotal' ? subtotal : lbl === 'VAT' ? vat : total;
                      const setter =
                        lbl === 'Subtotal' ? setSubtotal : lbl === 'VAT' ? setVat : setTotal;
                      return (
                        <Field key={lbl} label={lbl}>
                          <div className="relative">
                            <input
                              type="number"
                              step="0.01"
                              value={value}
                              onChange={(e) => setter(e.target.value)}
                              disabled={disabled}
                              className={NUMBER_CLS}
                              data-testid={`slip-field-${lbl.toLowerCase()}`}
                            />
                            {ocr.extractionState === 'running' && value === '0' ? (
                              <FieldSpinner />
                            ) : ocr.extractionState === 'done' && (Number(value) > 0 || value !== '0') ? (
                              <FilledTick filled />
                            ) : null}
                          </div>
                        </Field>
                      );
                    })}
                  </div>
                </div>
              </div>

              {ocr.pendingFile && ocr.extractionState === 'done' && !hideSubmitButton && (
                <div
                  className={`glass-tint-positive rounded-xl border p-4 space-y-1 ${
                    canConfirm ? 'border-positive/50 ' : 'border-rule-strong bg-paper-3/40'
                  }`}
                >
                  <button
                    type="button"
                    onClick={handleConfirm}
                    disabled={ocr.phase === 'confirming' || !canConfirm}
                    data-testid="slip-confirm"
                    className={`glass-panel w-full py-3 rounded-lg text-sm font-bold font-mono inline-flex items-center justify-center gap-2 disabled:opacity-50 disabled:cursor-not-allowed transition-colors ${
                      ocr.phase === 'confirming'
                        ? 'bg-rule-strong text-ink-2'
                        : canConfirm
                          ? 'bg-positive hover:bg-positive-strong text-paper'
                          : ' text-mute'
                    }`}
                  >
                    {ocr.phase === 'confirming'
                      ? '⏳ Saving…'
                      : canConfirm
                        ? '✓ Send & Confirm'
                        : '🔒 Send & Confirm (disabled)'}
                  </button>
                  <p className={`text-xs font-mono text-center ${canConfirm ? 'text-positive/70' : 'text-caution/80'}`}>
                    {ocr.phase === 'confirming'
                      ? 'Saving your expense as draft.'
                      : canConfirm
                        ? 'All required fields look good. Click to save as draft expense.'
                        : payment === 'transfer' && !bookBankSlipId
                          ? 'Transfer expenses require a book bank slip below.'
                          : 'Fill vendor + date; subtotal + VAT must equal total.'}
                  </p>
                </div>
              )}

              {ocr.pendingFile && ocr.extractionState === 'done' && (
                <div className="glass-panel rounded-xl overflow-hidden">
                  <details className="group" open>
                    <summary className="glass-panel-heavy flex items-center justify-between px-4 py-3 border-b border-rule cursor-pointer select-none hover:bg-paper-3/60 transition-colors [&::-webkit-details-marker]:hidden">
                      <span className="text-xs font-mono text-ink-2 uppercase tracking-widest font-semibold flex items-center gap-2">
                        <span className="text-positive text-xs">📋</span> OCR line items ({items.length})
                      </span>
                      <span className="text-xs text-mute font-mono transition-transform duration-200 group-open:rotate-180">
                        ▼
                      </span>
                    </summary>
                    <div className="p-3 space-y-3">
                      <div className="overflow-x-auto">
                        <table className="w-full text-left border-collapse text-sm font-mono">
                          <thead>
                            <tr className="border-b border-rule text-mute font-semibold uppercase tracking-wider text-sm">
                              <th className="py-1.5 px-2 pb-2 text-ink-2">Description</th>
                              <th className="py-1.5 px-2 pb-2 text-center w-20 text-ink-2">Qty</th>
                              <th className="py-1.5 px-2 pb-2 text-right w-24 text-ink-2">Unit Price</th>
                              <th className="py-1.5 px-2 pb-2 text-right w-28 text-ink-2">Amount</th>
                              <th className="py-1.5 px-2 pb-2 text-center w-10 text-ink-2"></th>
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
                                    className="glass-panel w-16 focus:border-rule-strong rounded px-1.5 py-1 text-center text-sm font-mono text-ink focus:outline-none"
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
                                    className="glass-panel w-20 focus:border-rule-strong rounded px-1.5 py-1 text-right text-sm font-mono text-ink focus:outline-none"
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
                                    className="glass-panel w-24 focus:border-rule-strong rounded px-1.5 py-1 text-right text-sm font-mono font-semibold text-positive focus:outline-none"
                                  />
                                </td>
                                <td className="py-2 px-1 text-center">
                                  <button
                                    type="button"
                                    onClick={() => setItems((arr) => arr.filter((_, idx) => idx !== i))}
                                    disabled={ocr.phase === 'confirming'}
                                    className="text-mute hover:text-critical transition-colors p-1"
                                    title="Remove Item"
                                  >
                                    ✕
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
                          className="glass-panel px-2.5 py-1 rounded hover:bg-rule-strong border border-rule-strong text-ink text-xs font-mono transition-colors"
                        >
                          + Add Item
                        </button>
                      </div>
                    </div>
                  </details>
                </div>
              )}

              {ocr.error && <p className="text-xs text-critical font-mono">⚠ {ocr.error}</p>}

              {ocr.pendingFile && ocr.extractionState === 'done' && ocr.selectedModelDesc && (
                <p className="text-xs text-mute italic">{ocr.selectedModelDesc}</p>
              )}

              <div className="flex flex-wrap items-center justify-between gap-2 pt-2 border-t border-rule">
                {ocr.pendingFile && ocr.slipId != null && ocr.extractionState !== 'running' ? (
                  <button
                    type="button"
                    onClick={ocr.removeFile}
                    disabled={ocr.phase === 'confirming'}
                    className="glass-tint-critical px-3 py-1.5 rounded-lg hover:bg-critical-soft text-critical text-sm font-mono disabled:opacity-50"
                    data-testid="slip-remove"
                  >
                    🗑 Remove (wrong upload)
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
                      className="glass-panel px-3 py-1.5 rounded-lg hover:bg-paper-3 text-ink border border-rule-strong text-sm font-mono disabled:opacity-50"
                    >
                      {ocr.pendingFile ? '↺ Pick another file' : '📂 Pick a file'}
                    </button>
                  )}
                  {ocr.pendingFile && ocr.extractionState === 'done' && (
                    <details className="relative group" data-testid="slip-vision-model-review">
                      <summary className="glass-panel list-none cursor-pointer flex items-center gap-1.5 px-2 py-1 rounded-lg hover:border-rule-strong text-xs font-mono text-ink-2 [&::-webkit-details-marker]:hidden">
                        <span className="text-ink-2">Model:</span>
                        <span className="text-white truncate max-w-[160px]">{ocr.selectedModel || '—'}</span>
                        <span className="text-mute">▾</span>
                      </summary>
                      <div className="glass-panel absolute right-0 top-full mt-1 z-20 w-[min(560px,90vw)] p-2 rounded-xl">
                        <div className="grid grid-cols-1 md:grid-cols-2 gap-2 max-h-[60vh] overflow-y-auto">
                          {ocr.visionModels.map((m) => (
                            <ModelCard
                              key={m.id}
                              m={m}
                              selected={m.name === ocr.selectedModel}
                              onSelect={ocr.pickModel}
                              testId={`slip-vision-model-review-${m.id}`}
                              disabled={ocr.phase === 'confirming'}
                            />
                          ))}
                        </div>
                      </div>
                    </details>
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
                    className="glass-panel shrink-0 rounded-lg hover:border-positive/50 transition-colors cursor-zoom-in overflow-hidden"
                    title="Click to enlarge"
                  >
                    <img src={ocr.preview} alt="preview" className="w-20 h-20 object-cover" />
                  </button>
                ) : (
                  <div className="glass-panel w-20 h-20 shrink-0 rounded-lg flex items-center justify-center text-2xl">
                    📄
                  </div>
                )}
                <div className="flex-1 min-w-0">
                  <p className="text-sm font-bold text-white truncate">{vendor || ocr.fileName}</p>
                  <p className="text-xs text-ink-2 font-mono">
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
                  className="text-sm font-mono text-info hover:text-white inline-flex items-center gap-1"
                >
                  🔗 Open in expense workflow
                </Link>
              </div>

              {!ocr.locked && (
                <div className="flex items-center justify-end gap-2 pt-2 border-t border-rule">
                  <button
                    type="button"
                    onClick={ocr.removeFile}
                    className="glass-tint-critical px-3 py-1.5 rounded-lg hover:bg-critical-soft text-critical text-sm font-mono"
                    data-testid="slip-remove-confirmed"
                  >
                    🗑 Remove (wrong upload)
                  </button>
                </div>
              )}
              {ocr.locked && ocr.lockReason && (
                <p className="text-xs font-mono text-mute border-t border-rule pt-2">
                  🔒 {ocr.lockReason}
                </p>
              )}

              <div className="flex items-center justify-end pt-1">
                <button
                  type="button"
                  onClick={ocr.pickAnother}
                  className="glass-panel px-3 py-1.5 rounded-lg hover:bg-paper-3 text-ink border border-rule-strong text-xs font-mono"
                >
                  + Upload another slip
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