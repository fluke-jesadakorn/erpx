'use client';

import React, { useState } from 'react';
import { useRouter } from 'next/navigation';
import { attachPaymentSlipAction } from '@/app/actions/expense';
import { SlipUpload } from '@/components/SlipUpload';
import type { VisionModel } from '@folio-lib/ai/loadVisionModels';

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
      setError('Upload the payment slip first.');
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
        setError(res.error ?? 'attach failed');
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
          Mark as <span className="font-mono">disbursed</span>: attach the payment slip and post to GL.
        </p>
      </div>

      <div>
        <label className="block text-xs font-mono uppercase tracking-wider text-cyan-300/80">
          Payment slip (required)
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
            Upload the payment receipt slip to enable the disbursement button.
          </p>
        )}
      </div>

      <div className="flex flex-wrap items-center gap-2">
        <label className="text-xs font-mono uppercase tracking-wider text-cyan-300/80">
          Payment method
        </label>
        <select
          name="paymentMethod"
          value={method}
          onChange={(e) => setMethod(e.target.value as typeof method)}
          disabled={busy}
          className="rounded-lg border border-slate-700 bg-slate-950 px-2 py-1.5 text-xs text-white disabled:opacity-50"
        >
          <option value="transfer">Bank transfer</option>
          <option value="cash">Cash</option>
          <option value="credit_card">Credit card</option>
        </select>
        <button
          type="submit"
          disabled={busy || slipId == null}
          className="rounded-lg bg-cyan-400 px-4 py-1.5 text-xs font-bold text-slate-950 hover:bg-cyan-300 disabled:opacity-50"
        >
          {busy ? 'Posting…' : '💸 Confirm disbursement'}
        </button>
      </div>

      {error && (
        <p className="rounded-md border border-rose-500/40 bg-rose-950/40 px-2 py-1 text-sm text-rose-200">
          {error}
        </p>
      )}
    </form>
  );
};

export default SettleForm;