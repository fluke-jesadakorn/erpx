'use client';

import React, { useRef, useState } from 'react';
import { useRouter, useSearchParams } from 'next/navigation';
import { attachPaymentSlipAction } from '@/app/actions/expense';
import { SlipUpload, type SlipUploadHandle } from '@/components/SlipUpload';
import type { VisionModel } from '@/ai/loadVisionModels';
import { T } from '@/components/i18n/T';
import type { ParsedFields, SubmitState } from '@/components/slips/types';
import { useTranslations } from 'next-intl';
import {
  ArrowUpRight,
  Banknote,
  Building2,
  CalendarDays,
  Check,
  FileText,
  Landmark,
  Loader2,
  Send,
  ShieldCheck,
  Sparkles,
  UserRound,
  Wand2,
} from 'lucide-react';

interface Props {
  waybillId: string;
  expenseId: number;
  visionModels?: VisionModel[];
  initialSlipId?: number;
}

interface PaymentExtraction {
  payer: string;
  payee: string;
  amount: number;
  paymentDate: string;
  bankName: string;
  accountNumber: string;
  reference: string;
  confidence: number | null;
}

const COMPANY = {
  name: 'Folio Systems (Thailand) Co., Ltd.',
  nameTh: 'บริษัท โฟลิโอ ซิสเต็มส์ (ประเทศไทย) จำกัด',
  taxId: '0105566123456',
  account: 'Kasikornbank · Corporate account · •••• 1234',
};

const DEMO_EXTRACTION: PaymentExtraction = {
  payer: COMPANY.name,
  payee: 'Northstar Office Supplies Co., Ltd.',
  amount: 24680,
  paymentDate: '2026-07-18',
  bankName: 'Kasikornbank · Corporate account',
  accountNumber: '•••• 1234',
  reference: 'PAY-00682',
  confidence: 98,
};

function extractionFromParsed(parsed: ParsedFields, waybillId: string): PaymentExtraction {
  return {
    payer: COMPANY.name,
    payee: parsed.payee ?? parsed.accountName ?? parsed.vendorName ?? parsed.createdTo ?? '—',
    amount: parsed.totalAmount ?? 0,
    paymentDate: parsed.transactionDate ?? '',
    bankName: parsed.bankName ?? '—',
    accountNumber: parsed.accountNumber ?? '—',
    reference: parsed.reference ?? `${waybillId} / payment slip`,
    confidence: null,
  };
}

function formatAmount(amount: number): string {
  if (!amount) return '—';
  return `฿${amount.toLocaleString('th-TH', { minimumFractionDigits: 2 })}`;
}

function MockPaymentSlip({ extraction }: { extraction: PaymentExtraction }) {
  return (
    <div className="rounded-xl border border-[#d9d4c8] bg-[#fbfaf7] p-3 text-[#182234] shadow-inner">
      <div className="flex items-start justify-between gap-3 border-b border-[#ddd8cc] pb-2">
        <div>
          <p className="text-[10px] font-black uppercase tracking-[0.2em] text-[#1d4f91]">Folio</p>
          <p className="mt-0.5 text-[9px] text-[#596575]">Corporate payment advice</p>
        </div>
        <div className="rounded bg-[#e8effa] px-2 py-1 text-[9px] font-bold uppercase tracking-wider text-[#1d4f91]">
          Payment slip
        </div>
      </div>
      <div className="mt-3 space-y-2 text-[10px]">
        <div>
          <p className="text-[8px] font-bold uppercase tracking-wider text-[#7b8490]">Paid from</p>
          <p className="mt-0.5 font-semibold">{extraction.payer}</p>
          <p className="text-[9px] text-[#596575]">Tax ID {COMPANY.taxId}</p>
        </div>
        <div className="grid grid-cols-2 gap-2">
          <div className="rounded border border-[#e5e0d6] bg-white/70 p-2">
            <p className="text-[8px] font-bold uppercase tracking-wider text-[#7b8490]">Paid to</p>
            <p className="mt-0.5 truncate font-semibold">{extraction.payee}</p>
          </div>
          <div className="rounded border border-[#e5e0d6] bg-white/70 p-2">
            <p className="text-[8px] font-bold uppercase tracking-wider text-[#7b8490]">Date</p>
            <p className="mt-0.5 font-semibold">{extraction.paymentDate || '—'}</p>
          </div>
        </div>
        <div className="flex items-end justify-between border-y border-[#ddd8cc] py-2">
          <span className="text-[8px] font-bold uppercase tracking-wider text-[#7b8490]">Transfer amount</span>
          <span className="text-base font-black tabular-nums">{formatAmount(extraction.amount)}</span>
        </div>
        <div className="flex justify-between gap-2 text-[9px] text-[#596575]">
          <span>{extraction.bankName}</span>
          <span className="font-semibold">{extraction.accountNumber}</span>
        </div>
        <div className="truncate border-t border-[#eee9df] pt-2 text-[8px] text-[#7b8490]">Ref. {extraction.reference}</div>
      </div>
    </div>
  );
}

function PaymentExtractionCard({
  extraction,
  isDemo,
  canExtract,
  extracting,
  sentToUser,
  onExtract,
  onSend,
}: {
  extraction: PaymentExtraction;
  isDemo: boolean;
  canExtract: boolean;
  extracting: boolean;
  sentToUser: boolean;
  onExtract: () => void;
  onSend: () => void;
}) {
  return (
    <section className="space-y-3 rounded-xl border border-accent/40 bg-accent-soft/20 p-3.5" data-testid="payslip-ai-extraction">
      <div className="flex items-start gap-3">
        <span className="grid size-9 shrink-0 place-items-center rounded-lg bg-accent text-paper-2 shadow-sm">
          <Sparkles className="size-4" aria-hidden strokeWidth={2.5} />
        </span>
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-2">
            <p className="text-[11px] font-mono font-bold uppercase tracking-[0.15em] text-accent-strong">
              AI extraction · ดึงข้อมูลอัตโนมัติ
            </p>
            <span className="rounded-full border border-positive/40 bg-positive-soft px-2 py-0.5 text-[10px] font-bold text-positive">
              {isDemo ? `Demo · ${extraction.confidence}%` : 'Review ready'}
            </span>
          </div>
          <h3 className="mt-1 text-sm font-semibold text-ink">Payment slip readback</h3>
          <p className="mt-0.5 text-xs leading-relaxed text-ink-2">
            {isDemo
              ? 'Static Folio sample — upload a real slip to replace these values.'
              : 'The extracted values are prefilled below. Review them before posting.'}
          </p>
        </div>
      </div>

      {isDemo && <MockPaymentSlip extraction={extraction} />}

      <div className="grid gap-2 sm:grid-cols-2">
        {[
          { label: 'From · ผู้จ่าย', value: extraction.payer, icon: Building2 },
          { label: 'To · ผู้รับเงิน', value: extraction.payee, icon: UserRound },
          { label: 'Amount · จำนวนเงิน', value: formatAmount(extraction.amount), icon: Banknote },
          { label: 'Date · วันที่โอน', value: extraction.paymentDate || '—', icon: CalendarDays },
          { label: 'Bank · ธนาคาร', value: extraction.bankName, icon: Landmark },
          { label: 'Account · เลขบัญชี', value: extraction.accountNumber, icon: Landmark },
          { label: 'Reference · เลขอ้างอิง', value: extraction.reference, icon: FileText },
        ].map(({ label, value, icon: Icon }) => (
          <div key={label} className="min-w-0 rounded-lg border border-rule/80 bg-paper-2/70 px-2.5 py-2">
            <div className="flex items-center gap-1.5 text-[10px] font-mono uppercase tracking-wider text-mute">
              <Icon className="size-3" aria-hidden strokeWidth={2} />
              {label}
            </div>
            <p className="mt-1 truncate text-xs font-semibold text-ink" title={value}>{value}</p>
          </div>
        ))}
      </div>

      <div className="flex flex-wrap items-center justify-between gap-2 border-t border-accent/20 pt-3">
        <div className="flex items-start gap-2 text-[11px] leading-relaxed text-ink-2">
          <ShieldCheck className="mt-0.5 size-3.5 shrink-0 text-positive" aria-hidden />
          <span>{isDemo ? 'Mock notification only · no external message is sent.' : 'Send the readback to the waybill submitter.'}</span>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <button
            type="button"
            onClick={onExtract}
            disabled={!canExtract || extracting}
            data-testid="payslip-extract-destination"
            className="inline-flex min-h-9 items-center gap-1.5 rounded-lg border border-accent/40 bg-paper-2 px-3 py-2 text-xs font-bold text-accent-strong transition-colors hover:bg-accent-soft disabled:cursor-not-allowed disabled:opacity-50"
          >
            {extracting ? <Loader2 className="size-3.5 animate-spin" aria-hidden /> : <Wand2 className="size-3.5" aria-hidden strokeWidth={2.5} />}
            {extracting ? 'Extracting…' : isDemo ? 'Extract payment destination' : 'Re-extract destination'}
          </button>
          <button
            type="button"
            onClick={onSend}
            disabled={sentToUser}
            data-testid="payslip-send-to-user"
            className={`inline-flex min-h-9 items-center gap-1.5 rounded-lg px-3 py-2 text-xs font-bold transition-colors ${
              sentToUser
                ? 'border border-positive/40 bg-positive-soft text-positive'
                : 'bg-accent text-paper-2 hover:bg-accent-strong'
            }`}
          >
            {sentToUser ? <Check className="size-3.5" aria-hidden strokeWidth={2.5} /> : <Send className="size-3.5" aria-hidden strokeWidth={2.5} />}
            {sentToUser ? 'Sent to user' : 'Send to user'}
            {!sentToUser && <ArrowUpRight className="size-3.5" aria-hidden strokeWidth={2.5} />}
          </button>
        </div>
      </div>
      {sentToUser && (
        <p className="rounded-lg border border-positive/30 bg-positive-soft/60 px-2.5 py-2 text-[11px] font-medium text-positive-strong">
          ✓ Readback prepared for the submitter · ผู้ส่งจะเห็นข้อมูลสลิปใน Waybill chat
        </p>
      )}
    </section>
  );
}

export const SettleForm: React.FC<Props> = ({ waybillId, expenseId, visionModels = [], initialSlipId }) => {
  const router = useRouter();
  const t = useTranslations();
  const search = useSearchParams();
  const slipRef = useRef<SlipUploadHandle>(null);
  const querySlip = Number(search?.get('slipId') ?? '');
  const seed = Number.isFinite(initialSlipId) && (initialSlipId as number) > 0
    ? (initialSlipId as number)
    : Number.isFinite(querySlip) && querySlip > 0
      ? querySlip
      : null;
  const [slipId, setSlipId] = useState<number | null>(seed);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [method, setMethod] = useState<'cash' | 'credit_card' | 'transfer'>('transfer');
  const [paymentDate, setPaymentDate] = useState(() => new Date().toISOString().slice(0, 10));
  const [amount, setAmount] = useState('');
  const [bankName, setBankName] = useState('');
  const [accountNumber, setAccountNumber] = useState('');
  const [payee, setPayee] = useState('');
  const [reference, setReference] = useState('');
  const [extraction, setExtraction] = useState<PaymentExtraction | null>(null);
  const [hasPendingSlip, setHasPendingSlip] = useState(false);
  const [extracting, setExtracting] = useState(false);
  const [sentToUser, setSentToUser] = useState(false);
  const demoExtraction = { ...DEMO_EXTRACTION, reference: `${waybillId} / ${DEMO_EXTRACTION.reference}` };

  async function onSubmit(e: React.FormEvent<HTMLFormElement>) {
    e.preventDefault();
    if (slipId == null) {
       setError('waybill.settle.uploadFirst');
      return;
    }
    setError(null);
    setBusy(true);
    try {
      const fd = new FormData(e.currentTarget);
      fd.set('waybillId', waybillId);
      fd.set('expenseId', String(expenseId));
      fd.set('slipId', String(slipId));
      fd.set('paymentMethod', method);
      fd.set('paymentDate', paymentDate);
      const res = await attachPaymentSlipAction(fd);
      if (!res.ok) {
         setError(res.error ?? 'waybill.settle.attachFailed');
        return;
      }
      router.push(`/waybill/${waybillId}`);
      router.refresh();
    } finally {
      setBusy(false);
    }
  }

  function onExtractionStateChange(state: SubmitState) {
    setHasPendingSlip(state.pendingFile);
    setExtracting(state.extractionState === 'running');
    if (state.pendingFile && state.extractionState === 'pending') {
      setSlipId(null);
      setExtraction(null);
      setSentToUser(false);
    }
  }

  function extractPaymentDestination() {
    if (!hasPendingSlip || extracting) return;
    setSentToUser(false);
    slipRef.current?.extract?.();
  }

  return (
    <form
      onSubmit={onSubmit}
      className="space-y-3 rounded-md border border-info/40 bg-info-soft p-4"
    >
      <div>
        <div className="flex items-start gap-3">
          <span className="grid size-9 shrink-0 place-items-center rounded-lg border border-info/40 bg-info text-paper">
            <Banknote className="size-4" aria-hidden strokeWidth={2.5} />
          </span>
          <div className="min-w-0 flex-1">
            <p className="text-[11px] font-mono font-bold uppercase tracking-[0.15em] text-info">
              Financial attachment · แนบหลักฐานการจ่าย
            </p>
            <h3 className="mt-1 text-base font-semibold text-ink"><T id="waybill.settle.markDisbursed" /></h3>
            <p className="mt-1 text-xs leading-relaxed text-ink-2">
              Attach the payment slip, let Folio AI read it back, then send the extracted summary to the user.
            </p>
          </div>
          <span className="shrink-0 rounded-full border border-caution/40 bg-caution-soft px-2 py-1 text-[10px] font-bold uppercase tracking-wider text-caution-strong">
            Required
          </span>
        </div>
      </div>

      <div className="rounded-xl border border-rule bg-paper/70 p-3">
        <div className="flex items-start gap-3">
          <span className="grid size-8 shrink-0 place-items-center rounded-lg bg-info-soft text-info">
            <Building2 className="size-4" aria-hidden strokeWidth={2.2} />
          </span>
          <div className="min-w-0 flex-1">
            <p className="text-[10px] font-mono font-bold uppercase tracking-[0.15em] text-mute">Our company · บริษัทของเรา</p>
            <p className="mt-1 text-sm font-semibold text-ink">{COMPANY.name}</p>
            <p className="text-xs text-ink-2">{COMPANY.nameTh}</p>
            <p className="mt-1 text-[11px] font-mono text-mute">Tax ID {COMPANY.taxId} · {COMPANY.account}</p>
          </div>
          <span className="inline-flex shrink-0 items-center gap-1 rounded-full border border-positive/40 bg-positive-soft px-2 py-1 text-[10px] font-bold text-positive">
            <ShieldCheck className="size-3" aria-hidden strokeWidth={2.5} /> Verified
          </span>
        </div>
      </div>

      <div>
        <label className="block text-xs font-mono uppercase tracking-wider text-info">
           <T id="waybill.settle.paymentSlipRequired" />
        </label>
        <div className="mt-1">
          <SlipUpload
            ref={slipRef}
            kind="receipt"
            currentUserId={0}
            onSlipReady={(id, _kind, parsed) => {
              setSlipId(id);
              setExtraction(extractionFromParsed(parsed, waybillId));
              setSentToUser(false);
              if (parsed.transactionDate) setPaymentDate(parsed.transactionDate);
              if (parsed.totalAmount != null) setAmount(String(parsed.totalAmount));
              if (parsed.bankName) setBankName(parsed.bankName);
              if (parsed.accountNumber) setAccountNumber(parsed.accountNumber);
              if (parsed.payee || parsed.accountName || parsed.vendorName) {
                setPayee(parsed.payee ?? parsed.accountName ?? parsed.vendorName ?? '');
              }
              if (parsed.reference) setReference(parsed.reference);
            }}
            onSlipDiscarded={() => {
              setSlipId(null);
              setExtraction(null);
              setHasPendingSlip(false);
              setExtracting(false);
              setSentToUser(false);
            }}
            onSubmitStateChange={onExtractionStateChange}
            hideSubmitButton
            initialModels={visionModels}
          />
        </div>
        {slipId == null && (
          <p className="mt-1 text-xs font-mono text-caution">
             <T id="waybill.settle.uploadReceipt" />
          </p>
        )}
      </div>

      <PaymentExtractionCard
        extraction={extraction ?? demoExtraction}
        isDemo={extraction == null}
        canExtract={hasPendingSlip}
        extracting={extracting}
        sentToUser={sentToUser}
        onExtract={extractPaymentDestination}
        onSend={() => setSentToUser(true)}
      />

      <div className="flex flex-wrap items-center gap-2">
        <label className="text-xs font-mono uppercase tracking-wider text-info">
           <T id="waybill.settle.paymentMethod" />
        </label>
        <select
          name="paymentMethod"
          value={method}
          onChange={(e) => setMethod(e.target.value as typeof method)}
          disabled={busy}
          className="rounded-lg border border-rule bg-paper px-2 py-1.5 text-xs text-ink disabled:opacity-50"
        >
           <option value="transfer">{t('waybill.settle.bankTransfer')}</option>
           <option value="cash">{t('waybill.settle.cash')}</option>
           <option value="credit_card">{t('waybill.settle.creditCard')}</option>
        </select>
        <button
          type="submit"
          data-testid="settle-submit"
          disabled={busy || slipId == null}
          className="rounded-lg bg-info px-4 py-1.5 text-xs font-bold text-ink hover:bg-info disabled:opacity-50"
        >
           {busy ? <T id="waybill.settle.posting" /> : <T id="waybill.settle.confirmDisbursement" />}
        </button>
      </div>

      <div className="grid gap-3 sm:grid-cols-2">
        <label className="text-xs font-mono uppercase tracking-wider text-info">
          Payment date
          <input name="paymentDate" type="date" required value={paymentDate} onChange={(e) => setPaymentDate(e.target.value)} className="mt-1 w-full rounded-md border border-rule bg-paper px-3 py-2 text-ink" />
        </label>
        <label className="text-xs font-mono uppercase tracking-wider text-info">
          Payment amount (document currency)
          <input name="amount" type="number" min="0.01" step="0.01" required value={amount} onChange={(e) => setAmount(e.target.value)} className="mt-1 w-full rounded-md border border-rule bg-paper px-3 py-2 text-ink" />
        </label>
        <label className="text-xs font-mono uppercase tracking-wider text-info">
          Bank or cash account
          <input name="bankName" required placeholder="Bank name or Cash" value={bankName} onChange={(e) => setBankName(e.target.value)} className="mt-1 w-full rounded-md border border-rule bg-paper px-3 py-2 text-ink" />
        </label>
        <label className="text-xs font-mono uppercase tracking-wider text-info">
          Account number
          <input name="accountNumber" placeholder="Optional" value={accountNumber} onChange={(e) => setAccountNumber(e.target.value)} className="mt-1 w-full rounded-md border border-rule bg-paper px-3 py-2 text-ink" />
        </label>
        <label className="text-xs font-mono uppercase tracking-wider text-info">
          Payee
          <input name="payee" required value={payee} onChange={(e) => setPayee(e.target.value)} className="mt-1 w-full rounded-md border border-rule bg-paper px-3 py-2 text-ink" />
        </label>
        <label className="text-xs font-mono uppercase tracking-wider text-info">
          Payment reference
          <input name="reference" required value={reference} onChange={(e) => setReference(e.target.value)} className="mt-1 w-full rounded-md border border-rule bg-paper px-3 py-2 text-ink" />
        </label>
      </div>

      {error && (
        <p className="rounded-md border border-critical/40 bg-critical-strong px-2 py-1 text-sm text-critical">
           {error?.startsWith('waybill.settle.') ? <T id={error} /> : error}
        </p>
      )}
    </form>
  );
};

export default SettleForm;
