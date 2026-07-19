'use client';

import React, { useState } from 'react';
import { useRouter } from 'next/navigation';
import {
  ArrowUpRight,
  Banknote,
  Building2,
  CalendarDays,
  FileCheck2,
  Landmark,
  Loader2,
  Paperclip,
  Send,
  ShieldCheck,
  UserRound,
} from 'lucide-react';
import { attachPaymentSlipAction } from '@/app/actions/expense';
import type { ExpensePaymentPreview } from '@/finance/expenseDocument';

interface Props {
  waybillId: string;
  expenseId: number;
  payment?: ExpensePaymentPreview | null;
}

function methodLabel(method: ExpensePaymentPreview['method']): string {
  if (method === 'credit_card') return 'Corporate card';
  if (method === 'transfer') return 'Bank transfer';
  return 'Cash payout';
}

function money(amount: number, currency: string): string {
  if (!Number.isFinite(amount) || amount <= 0) return '—';
  return `${amount.toLocaleString('th-TH', { minimumFractionDigits: 2, maximumFractionDigits: 2 })} ${currency}`;
}

function PaymentSlipPreview({
  payment,
  amount,
  paymentDate,
}: {
  payment: ExpensePaymentPreview;
  amount: number;
  paymentDate: string;
}) {
  return (
    <section
      className="overflow-hidden rounded-xl border border-positive/35 bg-paper-2 shadow-sm"
      data-testid="payslip-generated-preview"
    >
      <header className="flex flex-wrap items-start justify-between gap-3 bg-ink px-4 py-3 text-paper">
        <div>
          <p className="text-[10px] font-mono font-bold uppercase tracking-[0.18em] text-accent-soft">Folio payment document</p>
          <h4 className="mt-1 text-sm font-semibold">Simulated paid slip · สลิปจำลอง</h4>
        </div>
        <span className="rounded-full border border-positive/50 bg-positive px-2.5 py-1 text-[10px] font-black uppercase tracking-widest text-paper">
          Paid
        </span>
      </header>

      <div className="space-y-4 p-4">
        <div className="flex items-start gap-3">
          <span className="grid size-9 shrink-0 place-items-center rounded-lg bg-info-soft text-info">
            <Building2 className="size-4" aria-hidden />
          </span>
          <div className="min-w-0">
            <p className="text-[10px] font-mono uppercase tracking-wider text-mute">Paid from</p>
            <p className="mt-0.5 truncate text-sm font-semibold text-ink">{payment.companyName}</p>
            {payment.companyTaxId && <p className="text-xs text-ink-2">Tax ID {payment.companyTaxId}</p>}
          </div>
        </div>

        <div className="grid gap-2 sm:grid-cols-2">
          <div className="rounded-lg border border-rule bg-paper px-3 py-2.5">
            <p className="flex items-center gap-1.5 text-[10px] font-mono uppercase tracking-wider text-mute">
              <UserRound className="size-3" aria-hidden /> Paid to · ผู้รับเงิน
            </p>
            <p className="mt-1 truncate text-sm font-semibold text-ink" title={payment.payee}>{payment.payee || '—'}</p>
          </div>
          <div className="rounded-lg border border-rule bg-paper px-3 py-2.5">
            <p className="flex items-center gap-1.5 text-[10px] font-mono uppercase tracking-wider text-mute">
              <CalendarDays className="size-3" aria-hidden /> Payment date
            </p>
            <p className="mt-1 text-sm font-semibold text-ink">{paymentDate || '—'}</p>
          </div>
          <div className="rounded-lg border border-rule bg-paper px-3 py-2.5">
            <p className="flex items-center gap-1.5 text-[10px] font-mono uppercase tracking-wider text-mute">
              <Landmark className="size-3" aria-hidden /> Destination
            </p>
            <p className="mt-1 truncate text-sm font-semibold text-ink" title={payment.bankName}>{payment.bankName || '—'}</p>
          </div>
          <div className="rounded-lg border border-rule bg-paper px-3 py-2.5">
            <p className="flex items-center gap-1.5 text-[10px] font-mono uppercase tracking-wider text-mute">
              <ShieldCheck className="size-3" aria-hidden /> Account
            </p>
            <p className="mt-1 truncate font-mono text-sm font-semibold text-ink">{payment.accountNumber || methodLabel(payment.method)}</p>
          </div>
        </div>

        <div className="flex flex-wrap items-end justify-between gap-3 rounded-lg border border-positive/35 bg-positive-soft/60 px-4 py-3">
          <div>
            <p className="text-[10px] font-mono font-bold uppercase tracking-widest text-positive">Amount paid · จำนวนเงิน</p>
            <p className="mt-1 text-xs text-ink-2">From the approved payable</p>
          </div>
          <p className="font-mono text-2xl font-black tabular-nums text-positive-strong">{money(amount, payment.currency)}</p>
        </div>

        <div className="flex items-start gap-2 rounded-lg border border-caution/35 bg-caution-soft/60 px-3 py-2 text-xs leading-relaxed text-caution-strong">
          <FileCheck2 className="mt-0.5 size-4 shrink-0" aria-hidden />
          <span>The slip is simulated because no bank integration is connected. The payee, destination, amount, journal, and user notification are real Folio records.</span>
        </div>
      </div>
    </section>
  );
}

export const SettleForm: React.FC<Props> = ({ waybillId, expenseId, payment }) => {
  const router = useRouter();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(payment?.blocker ?? null);
  const [paymentDate, setPaymentDate] = useState(() => new Date().toISOString().slice(0, 10));
  const [amount, setAmount] = useState(() => payment?.amount ? String(payment.amount) : '');
  const amountNumber = Number(amount);
  const amountValid = !!payment
    && Number.isFinite(amountNumber)
    && amountNumber > 0
    && amountNumber <= payment.amount + 0.005;
  const canSubmit = !!payment?.ready && amountValid && paymentDate.length > 0 && !busy;

  async function onSubmit(e: React.FormEvent<HTMLFormElement>) {
    e.preventDefault();
    if (!canSubmit) return;
    setError(null);
    setBusy(true);
    try {
      const fd = new FormData(e.currentTarget);
      fd.set('waybillId', waybillId);
      fd.set('expenseId', String(expenseId));
      const res = await attachPaymentSlipAction(fd);
      if (!res.ok) {
        setError(res.error ?? 'Payment confirmation failed');
        return;
      }
      router.push(`/waybill/${waybillId}#expense-attachments`);
      router.refresh();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : 'Payment confirmation failed');
    } finally {
      setBusy(false);
    }
  }

  if (!payment) {
    return (
      <section className="rounded-xl border border-critical/40 bg-critical-soft p-4 text-sm text-critical-strong">
        Payment details are unavailable for this expense.
      </section>
    );
  }

  return (
    <form onSubmit={onSubmit} className="space-y-4 rounded-xl border border-info/40 bg-info-soft/30 p-4">
      <header className="flex items-start gap-3">
        <span className="grid size-10 shrink-0 place-items-center rounded-lg bg-info text-paper">
          <Banknote className="size-5" aria-hidden strokeWidth={2.5} />
        </span>
        <div className="min-w-0 flex-1">
          <p className="text-[10px] font-mono font-bold uppercase tracking-[0.16em] text-info">Final payment step · ขั้นตอนจ่ายเงิน</p>
          <h3 className="mt-1 text-base font-semibold text-ink">Record payment and attach the paid slip</h3>
          <p className="mt-1 text-xs leading-relaxed text-ink-2">
            Folio uses the approved payable and the submitter’s attachment. Confirming creates the simulated paid slip and sends it to the submitter’s notifications.
          </p>
        </div>
      </header>

      <div className="grid gap-3 sm:grid-cols-2">
        <label className="text-xs font-mono uppercase tracking-wider text-info">
          Payment date
          <input
            name="paymentDate"
            type="date"
            required
            value={paymentDate}
            onChange={(e) => setPaymentDate(e.target.value)}
            disabled={busy}
            className="mt-1 w-full rounded-lg border border-rule bg-paper px-3 py-2.5 text-ink disabled:opacity-50"
          />
        </label>
        <label className="text-xs font-mono uppercase tracking-wider text-info">
          Payment amount ({payment.currency})
          <input
            name="amount"
            type="number"
            min="0.01"
            max={payment.amount}
            step="0.01"
            required
            value={amount}
            onChange={(e) => setAmount(e.target.value)}
            disabled={busy}
            className="mt-1 w-full rounded-lg border border-rule bg-paper px-3 py-2.5 text-right font-mono text-ink disabled:opacity-50"
          />
        </label>
      </div>

      <div className="flex flex-wrap items-center gap-2 text-xs">
        <span className="rounded-full border border-rule bg-paper px-2.5 py-1 font-semibold text-ink-2">{methodLabel(payment.method)}</span>
        <span className="rounded-full border border-positive/35 bg-positive-soft px-2.5 py-1 font-semibold text-positive">
          Real payable: {money(payment.amount, payment.currency)}
        </span>
        {payment.sourceAttachmentKey && (
          <a
            href={`/api/slips/file?key=${encodeURIComponent(payment.sourceAttachmentKey)}`}
            target="_blank"
            rel="noreferrer"
            className="inline-flex items-center gap-1 rounded-full border border-info/35 bg-paper px-2.5 py-1 font-semibold text-info hover:border-info"
          >
            <Paperclip className="size-3" aria-hidden /> User bank attachment
          </a>
        )}
      </div>

      <PaymentSlipPreview payment={payment} amount={amountNumber} paymentDate={paymentDate} />

      {(!amountValid || error) && (
        <p role="alert" className="rounded-lg border border-critical/35 bg-critical-soft px-3 py-2 text-sm text-critical-strong">
          {error ?? `Payment must be between 0.01 and ${payment.amount.toFixed(2)} ${payment.currency}.`}
        </p>
      )}

      <button
        type="submit"
        data-testid="settle-submit"
        disabled={!canSubmit}
        className="inline-flex min-h-12 w-full items-center justify-center gap-2 rounded-lg bg-info px-4 py-3 text-sm font-bold text-paper transition hover:bg-info-strong disabled:cursor-not-allowed disabled:opacity-50"
      >
        {busy ? <Loader2 className="size-4 animate-spin" aria-hidden /> : <Send className="size-4" aria-hidden />}
        {busy ? 'Recording payment…' : 'Record payment & send paid slip'}
        {!busy && <ArrowUpRight className="size-4" aria-hidden />}
      </button>
    </form>
  );
};

export default SettleForm;
