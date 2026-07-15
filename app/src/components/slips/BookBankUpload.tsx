'use client';

import React, { forwardRef, useEffect, useImperativeHandle, useRef } from 'react';
import { Modal } from '@/components/ui/Modal';
import {
  FilledTick,
  Field,
  FieldSpinner,
  ModelCard,
  fileKind,
  formatBytes,
} from './SlipCard';
import { useSlipOcr } from './useSlipOcr';
import type {
  BookBankFields,
  ParsedFields,
  SlipUploadHandle,
} from './types';

export interface BookBankUploadProps {
  currentUserId?: number;
  initialModels?: Parameters<typeof useSlipOcr>[0]['initialModels'];
  onSlipReady?: (slipId: number, kind: 'book_bank') => void;
  onSlipDiscarded?: (slipId: number, kind: 'book_bank') => void;
  onBookBankFieldsChange?: (f: BookBankFields) => void;
  hideSubmitButton?: boolean;
  autoExtract?: boolean;
}

const INPUT_CLS =
  'glass-panel w-full rounded-xl hover:bg-paper-3/60 focus:border-positive/60 focus:ring-2 focus:ring-positive/20 transition-all px-3.5 py-2.5 pr-9 text-xs text-white focus:outline-none disabled:opacity-50 disabled:cursor-not-allowed placeholder-mute font-medium';

export const BookBankUpload = forwardRef<SlipUploadHandle, BookBankUploadProps>(
  function BookBankUpload(
    {
      currentUserId,
      initialModels,
      onSlipReady,
      onSlipDiscarded,
      onBookBankFieldsChange,
      autoExtract = true,
    },
    ref,
  ) {
    const ocr = useSlipOcr({
      kind: 'book_bank',
      initialModels,
      currentUserId,
      autoExtract,
      onSlipReady: (id) => onSlipReady?.(id, 'book_bank'),
      onSlipDiscarded: (id) => onSlipDiscarded?.(id, 'book_bank'),
    });

    const [bankName, setBankName] = React.useState('');
    const [bankBranch, setBankBranch] = React.useState('');
    const [accountNumber, setAccountNumber] = React.useState('');
    const [accountName, setAccountName] = React.useState('');
    const [dragOver, setDragOver] = React.useState(false);

    function applyParsed(p: ParsedFields) {
      const next: BookBankFields = {
        bankName: p.bankName ?? '',
        bankBranch: p.bankBranch ?? '',
        accountNumber: p.accountNumber ?? '',
        accountName: p.accountName ?? '',
      };
      setBankName(next.bankName);
      setBankBranch(next.bankBranch);
      setAccountNumber(next.accountNumber);
      setAccountName(next.accountName);
      onBookBankFieldsChange?.(next);
    }

    useEffect(() => {
      if (ocr.parsed) applyParsed(ocr.parsed);
      // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [ocr.parsed]);

    function emit(next: Partial<BookBankFields>) {
      const merged: BookBankFields = {
        bankName: next.bankName ?? bankName,
        bankBranch: next.bankBranch ?? bankBranch,
        accountNumber: next.accountNumber ?? accountNumber,
        accountName: next.accountName ?? accountName,
      };
      onBookBankFieldsChange?.(merged);
    }

    const disabled = !ocr.pendingFile || ocr.phase === 'confirming';
    const handleConfirmRef = useRef<() => Promise<void>>(() => Promise.resolve());
    useImperativeHandle(ref, () => ({ submit: () => handleConfirmRef.current() }), []);

    const confPct = Math.round(ocr.confidence * 100);
    const pendingKind = ocr.pendingFile ? fileKind(ocr.pendingFile.type, ocr.pendingFile.name) : '';

    return (
      <div className="space-y-3">
        <h3 className="text-xs font-bold text-ink uppercase tracking-wider flex items-center gap-2 font-mono">
          <span>📖</span>
          {ocr.phase === 'confirmed'
            ? 'Book Bank (Attached)'
            : 'Book Bank (Upload → OCR → Review → Attached on receipt confirm)'}
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
                  Drag &amp; drop a book bank slip
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
                    !ocr.pendingFile
                      ? 'bg-rule-strong/40 text-ink-2'
                      : ocr.extractionState === 'pending'
                        ? 'bg-rule-strong/40 text-ink-2'
                        : ocr.extractionState === 'running'
                          ? ' text-caution'
                          : 'bg-positive-soft text-positive'
                  }`}
                  data-testid="slip-step-badge"
                >
                  {!ocr.pendingFile && 'Step 1 · Pick model & file'}
                  {ocr.pendingFile && ocr.extractionState === 'pending' &&
                    'Step 2 · Ready to extract'}
                  {ocr.pendingFile && ocr.extractionState === 'running' &&
                    `Step 2 · Extracting · ${ocr.elapsed}s`}
                  {ocr.pendingFile && ocr.extractionState === 'done' && 'Step 2 · Review OCR'}
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

              {ocr.extractionState !== 'running' &&
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
              {ocr.extractionState !== 'running' &&
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
            </div>
          </div>

          {!ocr.pendingFile && (
            <div className="glass-panel rounded-xl border border-rule-strong p-4">
              <p className="text-sm font-mono text-center text-ink-2 py-3">
                Drop a book bank slip above or click to pick a file · Supports JPG / PNG / WEBP / PDF · Max 20 MB
              </p>
            </div>
          )}

          <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
            <Field label="Bank name">
              <div className="relative">
                <input
                  value={bankName}
                  onChange={(e) => {
                    setBankName(e.target.value);
                    emit({ bankName: e.target.value });
                  }}
                  placeholder="e.g. Krungthai, SCB, Kasikorn…"
                  disabled={disabled}
                  className={INPUT_CLS}
                />
                {ocr.extractionState === 'running' && !bankName && <FieldSpinner />}
                {ocr.extractionState === 'done' && bankName.trim() && <FilledTick filled />}
              </div>
            </Field>
            <Field label="Branch" hint="optional">
              <div className="relative">
                <input
                  value={bankBranch}
                  onChange={(e) => {
                    setBankBranch(e.target.value);
                    emit({ bankBranch: e.target.value });
                  }}
                  placeholder="e.g. 0080 สาขาฟิวเจอร์พาร์ค รังสิต"
                  disabled={disabled}
                  className={INPUT_CLS}
                />
                {ocr.extractionState === 'running' && !bankBranch && <FieldSpinner />}
                {ocr.extractionState === 'done' && bankBranch.trim() && <FilledTick filled />}
              </div>
            </Field>
            <Field label="Account number">
              <div className="relative">
                <input
                  inputMode="numeric"
                  value={accountNumber}
                  onChange={(e) => {
                    const v = e.target.value.replace(/[^\d]/g, '');
                    setAccountNumber(v);
                    emit({ accountNumber: v });
                  }}
                  placeholder="digits only"
                  disabled={disabled}
                  className={`${INPUT_CLS} font-mono`}
                />
                {ocr.extractionState === 'running' && !accountNumber && <FieldSpinner />}
                {ocr.extractionState === 'done' && accountNumber.trim() && <FilledTick filled />}
              </div>
            </Field>
            <Field label="Account name">
              <div className="relative">
                <input
                  value={accountName}
                  onChange={(e) => {
                    setAccountName(e.target.value);
                    emit({ accountName: e.target.value });
                  }}
                  placeholder="holder name as printed on the passbook"
                  disabled={disabled}
                  className={INPUT_CLS}
                />
                {ocr.extractionState === 'running' && !accountName && <FieldSpinner />}
                {ocr.extractionState === 'done' && accountName.trim() && <FilledTick filled />}
              </div>
            </Field>
          </div>

          {ocr.pendingFile && ocr.extractionState === 'done' && (
            <div className="glass-panel rounded-xl border border-rule-strong p-4 space-y-1">
              <p className="text-sm font-mono text-positive text-center">
                ✓ Book bank attached — SLIP-{ocr.slipId}
              </p>
              <p className="text-xs font-mono text-center text-ink-2">
                Will be linked to the expense when you submit the receipt.
              </p>
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
        </div>

        {ocr.preview && (
          <Modal
            open={ocr.zoomOpen}
            onClose={() => ocr.setZoomOpen(false)}
            title="Book bank preview"
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

export default BookBankUpload;