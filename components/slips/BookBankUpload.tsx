'use client';

import React, { forwardRef, useEffect, useImperativeHandle, useRef } from 'react';
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
  CircleCheck,
  CircleAlert,
  ZoomIn,
  Eye,
  FileSpreadsheet,
  Wand2,
} from 'lucide-react';
import { Modal } from '@/components/ui/Modal';
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
  ParsedFields,
  SlipUploadHandle,
} from './types';

export interface BookBankUploadProps {
  currentUserId?: number;
  initialModels?: Parameters<typeof useSlipOcr>[0]['initialModels'];
  onSlipReady?: (slipId: number, kind: 'book_bank', parsed: ParsedFields) => void;
  onSlipDiscarded?: (slipId: number, kind: 'book_bank') => void;
  onBookBankFieldsChange?: (f: BookBankFields) => void;
  hideSubmitButton?: boolean;
}

const INPUT_CLS =
  'bg-paper-2 border border-rule w-full rounded-md hover:bg-paper-3/60 focus:border-positive/60 focus:ring-2 focus:ring-positive/20 transition-all px-3.5 py-2.5 pr-9 text-xs text-ink focus:outline-none disabled:opacity-50 disabled:cursor-not-allowed placeholder-mute font-medium';

export const BookBankUpload = forwardRef<SlipUploadHandle, BookBankUploadProps>(
  function BookBankUpload(
    {
      currentUserId,
      initialModels,
      onSlipReady,
      onSlipDiscarded,
      onBookBankFieldsChange,
    },
    ref,
  ) {
    const ocr = useSlipOcr({
      kind: 'book_bank',
      initialModels,
      currentUserId,
      onSlipReady: (id, _kind, parsed) => onSlipReady?.(id, 'book_bank', parsed),
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
    useImperativeHandle(
      ref,
      () => ({
        submit: () => handleConfirmRef.current(),
        extract: ocr.extract,
      }),
      [ocr.extract],
    );

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
                aria-label="Drop a book bank slip or click to browse"
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
              <div className="w-full h-32 sm:h-44 rounded-md flex items-center justify-center border border-rule bg-paper-3">
                <FileSpreadsheet className="size-12 text-ink-2" strokeWidth={1.2} aria-hidden />
              </div>
            )}

            {ocr.pendingFile && (
              <div className="w-full min-w-0 space-y-2">
                <div className="flex items-center gap-2 flex-wrap">
                  <span
                    className={`px-2.5 py-1 rounded-md text-xs font-sans tabular-nums uppercase inline-flex items-center gap-1 font-bold ${
                      ocr.extractionState === 'pending'
                        ? 'bg-rule-strong/40 text-ink-2'
                        : ocr.extractionState === 'running'
                          ? 'bg-caution-soft text-caution border border-caution/40'
                          : 'bg-positive-soft text-positive border border-positive/40'
                    }`}
                    title={
                      ocr.extractionState === 'pending'
                        ? 'Ready to extract'
                        : ocr.extractionState === 'running'
                          ? `Extracting · ${ocr.elapsed}s`
                          : 'Review OCR'
                    }
                    data-testid="slip-step-badge"
                  >
                    {ocr.extractionState === 'running' ? (
                      <Loader2 className="size-3.5 animate-spin" strokeWidth={2.5} />
                    ) : ocr.extractionState === 'done' ? (
                      <CircleCheck className="size-3.5" strokeWidth={2.5} />
                    ) : (
                      <span className="font-sans tabular-nums">2/2</span>
                    )}
                    {ocr.extractionState === 'pending'
                      ? 'ready'
                      : ocr.extractionState === 'running'
                        ? <span className="tabular-nums">{ocr.elapsed}s</span>
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

                {ocr.pendingFile && ocr.extractionState !== 'running' && (
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
                    {ocr.visionModels.length === 0 && (
                      <span className="text-xs text-caution-strong">No vision models configured by IT.</span>
                    )}
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

          {ocr.pendingFile && ocr.extractionState !== 'pending' && (
            <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
            <Field
              label="Bank"
              hint="optional"
              icon={<Banknote className="size-3" aria-hidden strokeWidth={2} />}
            >
              <div className="relative">
                <input
                  value={bankName}
                  onChange={(e) => {
                    setBankName(e.target.value);
                    emit({ bankName: e.target.value });
                  }}
                  placeholder="e.g. Krungthai, SCB, Kasikorn"
                  disabled={disabled}
                  className={INPUT_CLS}
                />
                {ocr.extractionState === 'running' && !bankName && <FieldSpinner />}
                {ocr.extractionState === 'done' && bankName.trim() && <FilledTick filled />}
              </div>
            </Field>
            <Field
              label="Branch"
              hint="optional"
              icon={<Building2 className="size-3" aria-hidden strokeWidth={2} />}
            >
              <div className="relative">
                <input
                  value={bankBranch}
                  onChange={(e) => {
                    setBankBranch(e.target.value);
                    emit({ bankBranch: e.target.value });
                  }}
                  placeholder="optional branch"
                  disabled={disabled}
                  className={INPUT_CLS}
                />
                {ocr.extractionState === 'running' && !bankBranch && <FieldSpinner />}
                {ocr.extractionState === 'done' && bankBranch.trim() && <FilledTick filled />}
              </div>
            </Field>
            <Field
              label="Account"
              hint="digits"
              icon={<Hash className="size-3" aria-hidden strokeWidth={2} />}
            >
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
                  className={`${INPUT_CLS} font-sans tabular-nums`}
                />
                {ocr.extractionState === 'running' && !accountNumber && <FieldSpinner />}
                {ocr.extractionState === 'done' && accountNumber.trim() && <FilledTick filled />}
              </div>
            </Field>
            <Field
              label="Holder"
              hint="optional"
              icon={<User className="size-3" aria-hidden strokeWidth={2} />}
            >
              <div className="relative">
                <input
                  value={accountName}
                  onChange={(e) => {
                    setAccountName(e.target.value);
                    emit({ accountName: e.target.value });
                  }}
                  placeholder="holder name"
                  disabled={disabled}
                  className={INPUT_CLS}
                />
                {ocr.extractionState === 'running' && !accountName && <FieldSpinner />}
                {ocr.extractionState === 'done' && accountName.trim() && <FilledTick filled />}
              </div>
            </Field>
          </div>
          )}

          {ocr.pendingFile && ocr.extractionState === 'done' && (
            <div
              title={`Book bank attached · SLIP-${ocr.slipId} · links when receipt submits`}
              className="bg-paper-2 border border-rule rounded-lg border border-positive/40 bg-positive-soft/30 p-3 flex items-center gap-2"
            >
              <CircleCheck className="size-4 text-positive" strokeWidth={2.5} aria-hidden />
              <span className="text-xs font-sans tabular-nums text-positive inline-flex items-center gap-1">
                SLIP-{ocr.slipId}
                <span className="text-positive/70 normal-case font-normal tracking-normal">linked on receipt submit</span>
              </span>
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
                className="inline-flex items-center justify-center w-9 h-9 rounded-lg border border-critical/40 bg-critical-soft text-critical hover:bg-critical/15 transition-colors disabled:opacity-50"
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
                  className="inline-flex items-center justify-center w-9 h-9 rounded-lg border border-rule-strong bg-paper-3 hover:bg-paper-3/80 text-ink-2 transition-colors disabled:opacity-50"
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
            <div className="flex items-center justify-center rounded-md p-1 border border-rule bg-paper-2">
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
