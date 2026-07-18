'use client';

import React, { useState } from 'react';
import { useRouter, useSearchParams } from 'next/navigation';
import { attachPaymentSlipAction } from '@/app/actions/expense';
import { SlipUpload } from '@/components/SlipUpload';
import type { VisionModel } from '@/ai/loadVisionModels';
import { T } from '@/components/i18n/T';

interface Props {
  waybillId: string;
  expenseId: number;
  visionModels?: VisionModel[];
  initialSlipId?: number;
}

export const SettleForm: React.FC<Props> = ({ waybillId, expenseId, visionModels = [], initialSlipId }) => {
  const router = useRouter();
  const search = useSearchParams();
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

  return (
    <form
      onSubmit={onSubmit}
      className="space-y-3 rounded-md border border-info/40 bg-info-soft p-4"
    >
      <div>
        <p className="text-xs text-info">
           <T id="waybill.settle.markDisbursed" />
        </p>
      </div>

      <div>
        <label className="block text-xs font-mono uppercase tracking-wider text-info">
           <T id="waybill.settle.paymentSlipRequired" />
        </label>
        <div className="mt-1">
          <SlipUpload
            kind="receipt"
            currentUserId={0}
            onSlipReady={(id, _kind, parsed) => {
              setSlipId(id);
              if (parsed.transactionDate) setPaymentDate(parsed.transactionDate);
              if (parsed.totalAmount != null) setAmount(String(parsed.totalAmount));
              if (parsed.bankName) setBankName(parsed.bankName);
              if (parsed.accountNumber) setAccountNumber(parsed.accountNumber);
              if (parsed.payee || parsed.accountName || parsed.vendorName) {
                setPayee(parsed.payee ?? parsed.accountName ?? parsed.vendorName ?? '');
              }
              if (parsed.reference) setReference(parsed.reference);
            }}
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
           <option value="transfer"><T id="waybill.settle.bankTransfer" /></option>
           <option value="cash"><T id="waybill.settle.cash" /></option>
           <option value="credit_card"><T id="waybill.settle.creditCard" /></option>
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
          Confirmed amount (THB)
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
