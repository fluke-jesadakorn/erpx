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
            onSlipReady={(id) => setSlipId(id)}
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

      {error && (
        <p className="rounded-md border border-critical/40 bg-critical-strong px-2 py-1 text-sm text-critical">
           {error?.startsWith('waybill.settle.') ? <T id={error} /> : error}
        </p>
      )}
    </form>
  );
};

export default SettleForm;