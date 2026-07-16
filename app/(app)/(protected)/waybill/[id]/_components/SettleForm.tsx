'use client';

import React, { useState } from 'react';
import { useRouter } from 'next/navigation';
import { attachPaymentSlipAction } from '@/app/actions/expense';
import { SlipUpload } from '@/components/SlipUpload';
import type { VisionModel } from '@/ai/loadVisionModels';
import { T } from '@/components/i18n/T';

interface Props {
  waybillId: string;
  expenseId: number;
  visionModels?: VisionModel[];
}

export const SettleForm: React.FC<Props> = ({ waybillId, expenseId, visionModels = [] }) => {
  const router = useRouter();
  const [slipId, setSlipId] = useState<number | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [method, setMethod] = useState<'cash' | 'credit_card' | 'transfer'>('transfer');

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
      className="space-y-3 rounded-2xl border border-cyan-500/40 bg-cyan-950/30 p-4"
    >
      <div>
        <p className="text-xs text-cyan-100">
           <T id="waybill.settle.markDisbursed" />
        </p>
      </div>

      <div>
        <label className="block text-xs font-mono uppercase tracking-wider text-cyan-300/80">
           <T id="waybill.settle.paymentSlipRequired" />
        </label>
        <div className="mt-1">
          <SlipUpload
            kind="receipt"
            currentUserId={0}
            onSlipReady={(id) => setSlipId(id)}
            hideSubmitButton
            initialModels={visionModels}
          />
        </div>
        {slipId == null && (
          <p className="mt-1 text-xs font-mono text-amber-300">
             <T id="waybill.settle.uploadReceipt" />
          </p>
        )}
      </div>

      <div className="flex flex-wrap items-center gap-2">
        <label className="text-xs font-mono uppercase tracking-wider text-cyan-300/80">
           <T id="waybill.settle.paymentMethod" />
        </label>
        <select
          name="paymentMethod"
          value={method}
          onChange={(e) => setMethod(e.target.value as typeof method)}
          disabled={busy}
          className="rounded-lg border border-slate-700 bg-slate-950 px-2 py-1.5 text-xs text-white disabled:opacity-50"
        >
           <option value="transfer"><T id="waybill.settle.bankTransfer" /></option>
           <option value="cash"><T id="waybill.settle.cash" /></option>
           <option value="credit_card"><T id="waybill.settle.creditCard" /></option>
        </select>
        <button
          type="submit"
          disabled={busy || slipId == null}
          className="rounded-lg bg-cyan-400 px-4 py-1.5 text-xs font-bold text-slate-950 hover:bg-cyan-300 disabled:opacity-50"
        >
           {busy ? <T id="waybill.settle.posting" /> : <T id="waybill.settle.confirmDisbursement" />}
        </button>
      </div>

      {error && (
        <p className="rounded-md border border-rose-500/40 bg-rose-950/40 px-2 py-1 text-sm text-rose-200">
           {error?.startsWith('waybill.settle.') ? <T id={error} /> : error}
        </p>
      )}
    </form>
  );
};

export default SettleForm;