import React from 'react';
import type { SecondaryLocale } from '@folio-lib/server/locale';
import { T } from '@/components/i18n/T';
import {
  rejectWaybillAction,
  finalRejectWaybillAction,
} from '@/app/actions/waybill';

interface InlineActionFormProps {
  kind: 'reject' | 'final-reject';
  waybillId: string;
  stage: string;
  locale?: SecondaryLocale;
}

export function InlineActionForm({
  kind,
  waybillId,
  stage,
  locale,
}: InlineActionFormProps) {
  const localeSafe: SecondaryLocale = locale ?? 'th';
  const action = kind === 'reject' ? rejectWaybillAction : finalRejectWaybillAction;
  const heading =
    kind === 'reject'
      ? { en: 'Reason for rejection', th: 'เหตุผลปฏิเสธ', de: 'Ablehnungsgrund' }
      : { en: 'Reason for final rejection', th: 'เหตุผลปฏิเสธขั้นสุดท้าย', de: 'Endgültiger Ablehnungsgrund' };
  const subtitle =
    kind === 'reject'
      ? {
          en: 'Reject this stage — submitter can correct and resubmit.',
          th: 'ปฏิเสธที่ขั้นนี้ ผู้ส่งสามารถแก้ไขและส่งใหม่ได้',
          de: 'Diese Stufe ablehnen — Einreicher kann korrigieren und erneut senden.',
        }
      : {
          en: 'Final reject — closes the approval chain.',
          th: 'ปฏิเสธขั้นสุดท้าย ปิดทางการอนุมัติ',
          de: 'Endgültige Ablehnung — schließt die Genehmigungskette.',
        };
  const cta =
    kind === 'reject'
      ? { en: 'Confirm reject', th: 'ยืนยันปฏิเสธ', de: 'Ablehnung bestätigen' }
      : { en: 'Confirm final reject', th: 'ยืนยันปฏิเสธขั้นสุดท้าย', de: 'Endgültige Ablehnung bestätigen' };

  const reasonLabel =
    localeSafe === 'th'
      ? 'เหตุผล (≥ 5 ตัวอักษร):'
      : localeSafe === 'de'
        ? 'Grund (≥ 5 Zeichen):'
        : 'Reason (≥ 5 chars):';
  const reasonPlaceholder =
    localeSafe === 'th'
      ? 'อธิบายเหตุผลที่ปฏิเสธ...'
      : localeSafe === 'de'
        ? 'Beschreiben Sie, warum abgelehnt werden soll…'
        : 'Describe why this should be rejected…';
  const cancelLabel =
    localeSafe === 'th' ? 'ยกเลิก' : localeSafe === 'de' ? 'Abbrechen' : 'Cancel';
  const stagePrefix = localeSafe === 'th' ? 'ขั้น:' : localeSafe === 'de' ? 'Stufe:' : 'stage:';

  return (
    <section className="rounded-2xl border border-rose-500/50 bg-rose-950/30 p-5 shadow-lg shadow-rose-500/10">
      <header className="mb-3 flex flex-wrap items-center gap-2">
        <span
          aria-hidden
          className="grid h-9 w-9 place-items-center rounded-xl bg-gradient-to-br from-rose-500/40 to-rose-700/40 text-lg ring-1 ring-rose-400/40"
        >
          ✗
        </span>
        <div className="flex flex-col">
          <span className="text-base font-bold text-rose-100">
            <T value={heading} />
          </span>
          <span className="font-mono text-xs uppercase tracking-widest text-rose-300/70">
            <T value={subtitle} />
            {' · '}
            <span className="text-cyan-300">{waybillId}</span>
            {' · '}
            <span className="text-rose-200">{stagePrefix} {stage}</span>
          </span>
        </div>
      </header>
      <form action={action} className="space-y-3">
        <input type="hidden" name="waybillId" value={waybillId} />
        <input type="hidden" name="stage" value={stage} />
        <label className="block text-sm font-medium text-rose-200">
          {reasonLabel}
          <textarea
            name="reason"
            required
            minLength={5}
            autoFocus
            placeholder={reasonPlaceholder}
            className="mt-2 block w-full rounded-lg bg-slate-950 p-3 text-sm text-white ring-1 ring-rose-500/30 placeholder:text-slate-500 focus:outline-none focus:ring-2 focus:ring-rose-400/60"
            rows={4}
          />
        </label>
        <div className="flex flex-wrap gap-3">
          <button
            type="submit"
            className="inline-flex items-center gap-2 rounded-xl bg-rose-500 px-5 py-2.5 text-sm font-bold text-slate-950 shadow-lg shadow-rose-500/30 transition hover:bg-rose-400"
          >
            <span aria-hidden>✗</span>
            <span>
              <T value={cta} />
            </span>
          </button>
          <a
            href={`/waybill/${waybillId}`}
            className="inline-flex items-center gap-2 rounded-xl border border-slate-700 px-5 py-2.5 text-sm text-slate-300 hover:border-slate-500"
          >
            {cancelLabel}
          </a>
        </div>
      </form>
    </section>
  );
}
