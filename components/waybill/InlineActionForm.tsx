'use client';

import React from 'react';
import { T } from '@/components/i18n/T';
import {
  rejectWaybillAction,
  finalRejectWaybillAction,
} from '@/app/actions/waybill';

interface InlineActionFormProps {
  kind: 'reject' | 'final-reject';
  waybillId: string;
  stage: string;
  locale?: unknown;
  submitAction?: (formData: FormData) => Promise<void>;
}

export function InlineActionForm({
  kind,
  waybillId,
  stage,
  submitAction,
}: InlineActionFormProps) {
  const action = submitAction ?? (kind === 'reject' ? rejectWaybillAction : finalRejectWaybillAction);

  return (
    <section className="rounded-md border border-critical bg-critical-soft p-5 shadow-lg shadow-critical">
      <header className="mb-3 flex flex-wrap items-center gap-2">
        <span
          aria-hidden
          className="grid h-9 w-9 place-items-center rounded-md bg-critical-soft text-lg ring-1 ring-critical"
        >
          ✗
        </span>
        <div className="flex flex-col">
          <span className="text-base font-bold text-critical-soft">
            <T id={kind === 'reject' ? 'waybill.actions.reasonFinal' : 'waybill.actions.reasonFinalLast'} />
          </span>
          <span className="font-mono text-xs uppercase tracking-widest text-critical">
            <T id={kind === 'reject' ? 'waybill.actions.rejectSubtitle' : 'waybill.actions.finalRejectSubtitle'} />
            {' · '}
            <span className="text-info">{waybillId}</span>
            {' · '}
            <span className="text-critical-soft">
              <T id="waybill.attachment.stage" /> {stage}
            </span>
          </span>
        </div>
      </header>
      <form action={action} className="space-y-3">
        <input type="hidden" name="waybillId" value={waybillId} />
        <input type="hidden" name="stage" value={stage} />
        <label className="block text-sm font-medium text-critical-soft">
          <T id="waybill.actions.reasonMin5" />
          <textarea
            name="reason"
            required
            minLength={5}
            autoFocus
            className="mt-2 block w-full rounded-lg bg-paper p-3 text-sm text-ink ring-1 ring-critical placeholder:text-mute focus:outline-none focus:ring-2 focus:ring-critical"
            rows={4}
          />
        </label>
        <div className="flex flex-wrap gap-3">
          <button
            type="submit"
            className="inline-flex items-center gap-2 rounded-md bg-critical px-5 py-2.5 text-sm font-bold text-ink shadow-lg shadow-critical transition hover:bg-critical"
          >
            <span aria-hidden>✗</span>
            <span>
              <T id={kind === 'reject' ? 'waybill.pip.confirmReject' : 'waybill.pip.confirmFinalReject'} />
            </span>
          </button>
          <a
            href={`/waybill/${waybillId}`}
            className="inline-flex items-center gap-2 rounded-md border border-rule px-5 py-2.5 text-sm text-ink-2 hover:border-rule"
          >
            <T id="waybill.pip.cancel" />
          </a>
        </div>
      </form>
    </section>
  );
}
