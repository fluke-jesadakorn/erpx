import React from 'react';
import Link from 'next/link';
import type { PipState, WaybillStagePip } from '@folio-lib/waybill/derive';
import { stageRoleLabel, nextStageOf } from '@folio-lib/waybill/derive';
import { stageLabel as pipStageLabel } from '@folio-lib/waybill/labels';
import type { WaybillEventRow } from '@folio-lib/waybill/events';
import type { ApproverRow } from '@folio-lib/waybill/queries';
import type { VisionModel } from '@folio-lib/ai/loadVisionModels';
import type { SecondaryLocale } from '@folio-lib/server/locale';
import {
  approveWaybillAction,
  confirmGlRecordedAction,
  finalApproveWaybillAction,
  finalRejectWaybillAction,
  rejectWaybillAction,
  resubmitWaybillAction,
} from '@/app/actions/waybill';
import { SettleForm } from '@/app/(app)/(protected)/waybill/[id]/_components/SettleForm';
import { ZoneSection } from '../ZoneSection';
import { ApproversList, type ActedUserLite } from '../ApproversList';
import { AttachmentUpload } from '../AttachmentUpload';
import { roleDisplayBi, eventKindLabelBi, initialsOf } from '../ui';
import { formatDateServer } from '@/components/i18n/formattersServer';
import { bi } from '@/components/i18n/bi';

const ACTED_KIND_LABEL: Record<string, { en: string; th: string }> = {
  created: { en: 'Created by', th: 'สร้างโดย' },
  submitted: { en: 'Submitted by', th: 'ส่งโดย' },
  advanced: { en: 'Approved by', th: 'อนุมัติโดย' },
  settled: { en: 'Settled by', th: 'จ่ายโดย' },
  'posted-to-gl': { en: 'Posted to GL by', th: 'บันทึกบัญชีโดย' },
  'posted-to-gl-accrual': { en: 'Posted accrual by', th: 'บันทึกค้างจ่ายโดย' },
  'posted-to-gl-settlement': { en: 'Posted settlement by', th: 'บันทึกจ่ายสุทธิโดย' },
  'gl-confirmed': { en: 'GL confirmed by', th: 'ยืนยันบัญชีโดย' },
  'gl-confirmed-accrual': { en: 'Accrual confirmed by', th: 'ยืนยันค้างจ่ายโดย' },
  'gl-confirmed-settlement': { en: 'Settlement confirmed by', th: 'ยืนยันจ่ายสุทธิโดย' },
  'signed-off': { en: 'Signed off by', th: 'ลงนามโดย' },
  rejected: { en: 'Rejected by', th: 'ปฏิเสธโดย' },
};

function actedKindLabel(kind: string, locale: SecondaryLocale): string {
  const entry = ACTED_KIND_LABEL[kind];
  if (!entry) {
    return bi('Acted by', 'ดำเนินการโดย', undefined, locale);
  }
  return bi(entry.en, entry.th, undefined, locale);
}

function roleFallbackLabel(kind: string, locale: SecondaryLocale): string | null {
  if (kind === 'created' || kind === 'submitted') {
    return bi('Submitter', 'ผู้ส่งเรื่อง', undefined, locale);
  }
  return null;
}

function pickActor(actedUsers: ActedUserLite[]): ActedUserLite | null {
  if (actedUsers.length === 0) return null;
  return actedUsers[actedUsers.length - 1];
}

function approverNames(approvers: ApproverRow[]): string[] {
  return approvers.slice(0, 3).map((a) => a.fullname);
}

const FINAL_APPROVAL_BG = 'border-fuchsia-500/30 bg-fuchsia-950/30';
const FINAL_APPROVAL_TEXT = 'text-fuchsia-200';

interface Props {
  waybillId: string;
  pip: WaybillStagePip;
  pipKey: string;
  state: PipState;
  isCurrentStage: boolean;
  isRejection: boolean;
  isPassed: boolean;
  isFinalApproval: boolean;
  isDisbursed: boolean;
  isAwaitingDisbursement: boolean;
  canAct: boolean;
  canAttach: boolean;
  canSettle: boolean;
  canFinalApprove: boolean;
  canConfirmGl: boolean;
  canReCall: boolean;
  hasGlConfirmed: boolean;
  originId: number | null;
  approvers: ApproverRow[];
  rejectionReason: string | null;
  rejectionActor: { user_id: number; fullname: string; role: string | null } | null;
  visionModels?: VisionModel[];
  events: WaybillEventRow[];
  actedUsers: ActedUserLite[];
  action: string | null;
  actionStage: string | null;
  isSubmitter: boolean;
  domain?: import('@folio-lib/waybill/derive').WaybillDomain;
  locale?: SecondaryLocale;
}

export function PipActionPrompt({
  waybillId,
  pip: _pip,
  pipKey,
  isCurrentStage,
  isRejection,
  isPassed,
  isFinalApproval,
  isDisbursed,
  isAwaitingDisbursement,
  canAct,
  canAttach,
  canSettle,
  canFinalApprove,
  canConfirmGl,
  canReCall,
  hasGlConfirmed: _hasGlConfirmed,
  originId,
  approvers,
  rejectionReason,
  rejectionActor,
  visionModels = [],
  events: _events,
  actedUsers,
  action,
  actionStage,
  isSubmitter,
  domain = 'expense',
  locale,
}: Props) {
  const localeSafe: SecondaryLocale = locale ?? 'th';
  if (isRejection) {
    return (
      <section className="space-y-3 border-t border-rose-500/30 pt-4">
        <div className="text-xs font-mono uppercase tracking-widest text-rose-200/80">
          ✗ {bi('Rejected by', 'ปฏิเสธโดย', undefined, localeSafe)}
        </div>
        {rejectionActor ? (
          <div className="rounded-lg border border-rose-500/40 bg-rose-950/30 p-3 text-sm text-rose-100">
            <span className="font-mono">
              {rejectionActor.fullname}
              <span className="ml-1 text-rose-300/80">#{rejectionActor.user_id}</span>
            </span>
            {rejectionActor.role && (
              <span className="ml-2 text-rose-300/80">
                ({roleDisplayBi(rejectionActor.role, localeSafe)})
              </span>
            )}
            {rejectionReason && (
              <p className="mt-2 italic text-rose-100">&ldquo;{rejectionReason}&rdquo;</p>
            )}
          </div>
        ) : (
          <p className="text-sm text-rose-100">
            {bi('no rejection event recorded', 'ไม่มีเหตุการณ์การปฏิเสธ', undefined, localeSafe)}
          </p>
        )}
        <ApproversList
          approvers={approvers}
          locale={localeSafe}
          tone="rose"
          currentUserId={null}
          title={bi('Original approvers', 'ผู้อนุมัติเดิม', undefined, localeSafe)}
        />
        {isSubmitter && (
          <form action={resubmitWaybillAction} className="rounded-2xl border border-amber-500/50 bg-amber-950/30 p-5">
            <input type="hidden" name="waybillId" value={waybillId} />
            <p className="text-sm text-amber-100">
              {bi(
                'Rejected. Resubmit and route the chain again from the start?',
                'ถูกปฏิเสธ — ส่งใหม่และเริ่มสายอนุมัติตั้งแต่ต้นหรือไม่?',
                undefined,
                localeSafe,
              )}
            </p>
            <button
              type="submit"
              className="mt-4 rounded-xl bg-amber-400 px-5 py-3 text-base font-bold text-slate-950 shadow-lg shadow-amber-500/30 transition hover:bg-amber-300"
            >
              ↩ {bi('Resubmit', 'ส่งใหม่', undefined, localeSafe)}
            </button>
          </form>
        )}
      </section>
    );
  }

  if (isCurrentStage && isFinalApproval && canFinalApprove) {
    const showFinalRejectForm = action === 'final-reject' && actionStage === pipKey;
    return (
      <section className={'space-y-3 border-t pt-4 ' + FINAL_APPROVAL_BG}>
        <div className="flex flex-wrap items-baseline justify-between gap-2 text-xs font-mono uppercase tracking-widest">
          <span className={FINAL_APPROVAL_TEXT}>
            🔒 {bi('Final Authorization', 'อนุมัติขั้นสุดท้าย', undefined, localeSafe)}
            <span className="ml-2 text-slate-400">
              {bi('current stage', 'ขั้นปัจจุบัน', undefined, localeSafe)}
            </span>
          </span>
          <span className="rounded-md border border-fuchsia-400/40 bg-fuchsia-500/15 px-2 py-0.5 text-fuchsia-200">
            {bi(
              'approve = GL post · reject = no GL',
              'อนุมัติ = บันทึกบัญชี · ปฏิเสธ = ไม่บันทึก',
              undefined,
              localeSafe,
            )}
          </span>
        </div>
        {canAttach && <AttachmentUpload waybillId={waybillId} stage={pipKey} />}
        {showFinalRejectForm && (
          <form action={finalRejectWaybillAction} className="rounded-2xl border border-rose-500/50 bg-rose-950/30 p-5">
            <input type="hidden" name="waybillId" value={waybillId} />
            <div className="mb-3 inline-flex items-center gap-2 rounded-md border border-rose-400/40 bg-rose-500/10 px-2 py-1 text-xs font-mono uppercase tracking-widest text-rose-200">
              🔒 {bi('Final reject · no GL post', 'ปฏิเสธขั้นสุดท้าย · ไม่บันทึกบัญชี', undefined, localeSafe)}
            </div>
            <label className="block text-sm font-medium text-rose-200">
              {bi('Reject reason (≥ 5 chars):', 'เหตุผลปฏิเสธ (≥ 5 ตัวอักษร):', undefined, localeSafe)}
              <textarea
                name="reason"
                required
                minLength={5}
                className="mt-2 block w-full rounded-lg bg-slate-950 p-3 text-sm text-white ring-1 ring-rose-500/30 focus:outline-none focus:ring-2 focus:ring-rose-400/60"
                rows={3}
              />
            </label>
            <div className="mt-4 flex gap-3">
              <button
                type="submit"
                className="rounded-xl bg-rose-500 px-5 py-3 text-base font-bold text-slate-950 shadow-lg shadow-rose-500/30 transition hover:bg-rose-400"
              >
                ✗ {bi('Confirm final reject', 'ยืนยันปฏิเสธขั้นสุดท้าย', undefined, localeSafe)}
              </button>
              <a
                href={`/waybill/${waybillId}`}
                className="rounded-xl border border-slate-700 px-5 py-3 text-base text-slate-300 hover:border-slate-500"
              >
                {bi('Cancel', 'ยกเลิก', undefined, localeSafe)}
              </a>
            </div>
          </form>
        )}
        <div className="grid gap-3 sm:grid-cols-2">
          <form action={finalApproveWaybillAction}>
            <input type="hidden" name="waybillId" value={waybillId} />
            <button
              type="submit"
              data-testid={`panel-final-approve-${waybillId}`}
              className="group inline-flex w-full flex-col items-center justify-center gap-1 rounded-2xl bg-gradient-to-br from-emerald-400 to-cyan-500 px-5 py-5 text-lg font-bold text-slate-950 shadow-xl shadow-emerald-500/40 transition hover:from-emerald-300 hover:to-cyan-400 hover:shadow-emerald-500/50"
            >
              <span className="flex items-center gap-2">
                <span aria-hidden className="text-2xl">✓</span>
                <span>{bi('Final approve', 'อนุมัติขั้นสุดท้าย', undefined, localeSafe)}</span>
              </span>
              <span className="text-xs font-mono uppercase tracking-widest text-emerald-950/70 group-hover:text-emerald-950">
                {bi('→ posts to GL', '→ บันทึกบัญชี (GL)', undefined, localeSafe)}
              </span>
            </button>
          </form>
          {!showFinalRejectForm && (
            <Link
              href={`/waybill/${waybillId}?action=final-reject&stage=${pipKey}`}
              data-testid={`panel-final-reject-${waybillId}`}
              className="group inline-flex flex-col items-center justify-center gap-1 rounded-2xl bg-gradient-to-br from-rose-400 to-rose-600 px-5 py-5 text-lg font-bold text-slate-950 shadow-xl shadow-rose-500/40 transition hover:from-rose-300 hover:to-rose-500 hover:shadow-rose-500/50"
            >
              <span className="flex items-center gap-2">
                <span aria-hidden className="text-2xl">✗</span>
                <span>{bi('Final reject', 'ปฏิเสธขั้นสุดท้าย', undefined, localeSafe)}</span>
              </span>
              <span className="text-xs font-mono uppercase tracking-widest text-rose-950/70 group-hover:text-rose-950">
                {bi('→ no GL post', '→ ไม่บันทึกบัญชี', undefined, localeSafe)}
              </span>
            </Link>
          )}
        </div>
        <ApproversList
          approvers={approvers}
          locale={localeSafe}
          tone="cyan"
          currentUserId={null}
          title={bi('Final approvers', 'ผู้อนุมัติขั้นสุดท้าย', undefined, localeSafe)}
        />
      </section>
    );
  }

  if (isCurrentStage && isDisbursed && canConfirmGl) {
    return (
      <section className="space-y-3 border-t border-emerald-500/30 pt-4">
        <div className="text-xs font-mono uppercase tracking-widest text-emerald-300">
          ✅ {bi('Confirm GL recorded', 'ยืนยันการบันทึกบัญชี', undefined, localeSafe)}
          <span className="ml-2 text-slate-400">
            {bi('current stage', 'ขั้นปัจจุบัน', undefined, localeSafe)}
          </span>
        </div>
        {originId != null ? (
          <form
            action={confirmGlRecordedAction}
            className="rounded-xl border border-amber-500/50 bg-amber-950/30 p-4 text-sm text-amber-100"
          >
            <input type="hidden" name="waybillId" value={waybillId} />
            <input type="hidden" name="expenseId" value={originId} />
            <p className="mb-3 font-mono">
              {bi(
                'GL posted — any accounting/finance officer can click to confirm.',
                'บันชีแล้ว — เจ้าหน้าที่บัญชี/การเงินท่านใดกดยืนยันก็ได้',
                undefined,
                localeSafe,
              )}
            </p>
            <button
              type="submit"
              data-testid={`panel-gl-confirm-${waybillId}`}
              className="inline-flex w-full items-center justify-center gap-2 rounded-xl bg-gradient-to-br from-amber-400 to-amber-500 px-5 py-5 text-lg font-bold text-slate-950 shadow-lg shadow-amber-500/40 transition hover:from-amber-300 hover:to-amber-400 hover:shadow-amber-500/50"
            >
              <span aria-hidden className="text-2xl">✓</span>
              <span>{bi('Confirm GL recorded', 'ยืนยันการบันทึกบัญชี', undefined, localeSafe)}</span>
            </button>
          </form>
        ) : (
          <p className="text-sm font-mono text-slate-400">
            {bi(
              'Waiting for the GL post from the previous step.',
              'รอการบันทึกบัญชี (GL) จากขั้นตอนก่อนหน้า',
              undefined,
              localeSafe,
            )}
          </p>
        )}
        <ApproversList
          approvers={approvers}
          locale={localeSafe}
          tone="emerald"
          currentUserId={null}
          title={bi('Confirmer approvers', 'ผู้ลงนามยืนยัน', undefined, localeSafe)}
        />
      </section>
    );
  }

  if (isCurrentStage && isAwaitingDisbursement && canSettle) {
    return (
      <section className="space-y-3 border-t border-cyan-500/30 pt-4">
        <div className="text-xs font-mono uppercase tracking-widest text-cyan-300">
          💸 {bi('Disbursement step', 'ขั้นตอนการจ่ายเงิน', undefined, localeSafe)}
          <span className="ml-2 text-slate-400">
            {bi('current stage', 'ขั้นปัจจุบัน', undefined, localeSafe)}
          </span>
        </div>
        {originId != null ? (
          <SettleForm waybillId={waybillId} expenseId={originId} visionModels={visionModels} />
        ) : (
          <p className="text-sm font-mono text-slate-400">
            {bi('no expenseId available to settle', 'ไม่พบ expenseId สำหรับจ่าย', undefined, localeSafe)}
          </p>
        )}
        <ApproversList
          approvers={approvers}
          locale={localeSafe}
          tone="cyan"
          currentUserId={null}
          title={bi('Disbursers', 'ผู้จ่ายเงิน', undefined, localeSafe)}
        />
      </section>
    );
  }

  if (isCurrentStage && canAct) {
    const roleText = bi(
      stageRoleLabel(pipKey, 'en'),
      stageRoleLabel(pipKey, 'th'),
      stageRoleLabel(pipKey, 'de'),
      localeSafe,
    );
    const showRejectForm = action === 'reject' && actionStage === pipKey;
    const nextStage = nextStageOf(domain, pipKey);
    const nextStageEn = nextStage ? pipStageLabel(nextStage, domain, 'en') : null;
    const nextStageTh = nextStage ? pipStageLabel(nextStage, domain, 'th') : null;
    const eligibleNames = approverNames(approvers);
    return (
      <section className="space-y-7 rounded-2xl border border-cyan-500/30 bg-gradient-to-br from-cyan-950/35 via-slate-950/40 to-slate-950/30 p-6 shadow-[0_0_0_1px_rgba(6,182,212,0.12),0_8px_28px_-10px_rgba(6,182,212,0.3)]">
        <div className="flex flex-wrap items-center gap-2 text-xs font-mono uppercase tracking-widest">
          <span aria-hidden className="inline-flex h-5 w-5 items-center justify-center rounded-full bg-cyan-500/20 text-cyan-300">
            ⚡
          </span>
          <span className="text-cyan-200">
            {bi('Acting now', 'กำลังดำเนินการ', undefined, localeSafe)}
          </span>
          <span className="text-slate-600">·</span>
          <span className="text-slate-400">
            {bi('current stage', 'ขั้นปัจจุบัน', undefined, localeSafe)}
          </span>
          <span className="ml-auto inline-flex items-center gap-1.5 rounded-full border border-cyan-500/40 bg-cyan-500/15 px-2 py-0.5 font-mono font-bold uppercase tracking-widest text-cyan-200">
            <span className="text-cyan-300/80">role:</span>
            <span className="text-cyan-100">{roleText}</span>
          </span>
        </div>
        {eligibleNames.length > 0 && (
          <div className="flex flex-wrap items-center gap-2 rounded-lg border border-cyan-500/20 bg-cyan-950/20 px-3 py-2 text-sm text-cyan-100">
            <span aria-hidden className="text-cyan-300">👤</span>
            <span className="font-mono uppercase tracking-widest text-cyan-300/80">
              {bi('Eligible approvers', 'ผู้ที่สามารถอนุมัติ', undefined, localeSafe)}
            </span>
            <span className="text-slate-500">·</span>
            <span className="text-cyan-50">
              {eligibleNames.join(', ')}
              {approvers.length > eligibleNames.length && (
                <span className="text-slate-400">
                  {' '}
                  {bi(
                    `+${approvers.length - eligibleNames.length} more`,
                    `+อีก ${approvers.length - eligibleNames.length} คน`,
                    undefined,
                    localeSafe,
                  )}
                </span>
              )}
            </span>
          </div>
        )}
        {canAttach && <AttachmentUpload waybillId={waybillId} stage={pipKey} />}
        {showRejectForm && (
          <form action={rejectWaybillAction} className="rounded-2xl border border-rose-500/40 bg-rose-950/30 p-5">
            <input type="hidden" name="waybillId" value={waybillId} />
            <input type="hidden" name="stage" value={pipKey} />
            <label className="block text-sm font-medium text-rose-200">
              {bi('Reject reason (≥ 5 chars):', 'เหตุผลปฏิเสธ (≥ 5 ตัวอักษร):', undefined, localeSafe)}
              <textarea
                name="reason"
                required
                minLength={5}
                className="mt-2 block w-full rounded-lg bg-slate-950 p-3 text-sm text-white ring-1 ring-rose-500/30 focus:outline-none focus:ring-2 focus:ring-rose-400/60"
                rows={3}
              />
            </label>
            <div className="mt-4 flex gap-3">
              <button
                type="submit"
                className="rounded-xl bg-rose-500 px-5 py-3 text-base font-bold text-slate-950 shadow-lg shadow-rose-500/30 transition hover:bg-rose-400"
              >
                ✗ {bi('Confirm reject', 'ยืนยันปฏิเสธ', undefined, localeSafe)}
              </button>
              <a
                href={`/waybill/${waybillId}`}
                className="rounded-xl border border-slate-700 px-5 py-3 text-base text-slate-300 hover:border-slate-500"
              >
                {bi('Cancel', 'ยกเลิก', undefined, localeSafe)}
              </a>
            </div>
          </form>
        )}
        <div className="grid gap-3 sm:grid-cols-[2.2fr_1fr]">
          <form action={approveWaybillAction}>
            <input type="hidden" name="waybillId" value={waybillId} />
            <input type="hidden" name="stage" value={pipKey} />
            <button
              type="submit"
              data-testid={`panel-approve-${pipKey}`}
              className="group relative inline-flex w-full flex-col items-center justify-center gap-1.5 overflow-hidden rounded-2xl bg-gradient-to-br from-cyan-300 via-cyan-400 to-cyan-600 px-5 py-5 text-lg font-bold text-slate-950 shadow-[0_10px_28px_-8px_rgba(6,182,212,0.6),inset_0_1px_0_rgba(255,255,255,0.45)] transition hover:from-cyan-200 hover:via-cyan-300 hover:to-cyan-500 hover:shadow-[0_12px_32px_-8px_rgba(6,182,212,0.75),inset_0_1px_0_rgba(255,255,255,0.55)]"
            >
              <span className="flex items-center gap-2">
                <span aria-hidden className="text-2xl leading-none">✓</span>
                <span>{bi('Approve', 'อนุมัติ', undefined, localeSafe)}</span>
              </span>
              {nextStageEn ? (
                <span className="inline-flex items-center gap-1 text-xs font-mono uppercase tracking-widest text-cyan-950/80 group-hover:text-cyan-950">
                  <span>{bi('advance to', 'ไปขั้น', undefined, localeSafe)}</span>
                  <span className="text-cyan-950/90">{nextStageEn.emoji}</span>
                  <span className="text-cyan-950/90">
                    {bi(nextStageEn.label, nextStageTh?.label, undefined, localeSafe)}
                  </span>
                </span>
              ) : (
                <span className="text-xs font-mono uppercase tracking-widest text-cyan-950/80 group-hover:text-cyan-950">
                  {bi('→ close this step', '→ ปิดขั้นตอน', undefined, localeSafe)}
                </span>
              )}
            </button>
          </form>
          {!showRejectForm && (
            <Link
              href={`/waybill/${waybillId}?action=reject&stage=${pipKey}`}
              data-testid={`panel-reject-${pipKey}`}
              className="group inline-flex items-center justify-center gap-2 rounded-2xl border border-slate-700/80 bg-slate-900/60 px-4 py-4 text-sm font-bold text-slate-300 shadow-inner transition hover:border-rose-500/40 hover:bg-rose-950/20 hover:text-rose-200"
            >
              <span aria-hidden className="text-lg leading-none">✗</span>
              <span>{bi('Reject', 'ปฏิเสธ', undefined, localeSafe)}</span>
            </Link>
          )}
        </div>
        <div className="mt-2 border-t border-cyan-500/15 pt-5">
          <ApproversList
            approvers={approvers}
            locale={localeSafe}
            tone="cyan"
            currentUserId={null}
            title={bi('Approvers', 'ผู้อนุมัติ', undefined, localeSafe)}
          />
        </div>
      </section>
    );
  }

  if (isPassed) {
    const actor = pickActor(actedUsers);
    const label = actor
      ? actedKindLabel(actor.kind, localeSafe)
      : bi('Completed by', 'เสร็จสิ้นโดย', undefined, localeSafe);
    return (
      <section className="space-y-3 border-t border-emerald-500/30 pt-4">
        <div className="text-xs font-mono uppercase tracking-widest text-emerald-200/80">
          ✓ {label}
        </div>
        {actor ? (
          <div className="flex items-center gap-3 rounded-lg border border-emerald-500/30 bg-emerald-950/30 p-3 text-sm text-emerald-100">
            <span
              aria-hidden
              className="inline-flex h-9 w-9 shrink-0 items-center justify-center rounded-full border border-emerald-400/40 bg-emerald-500/20 text-xs font-bold uppercase text-emerald-100"
            >
              {initialsOf(actor.fullname)}
            </span>
            <div className="min-w-0 flex-1">
              <div className="truncate font-semibold text-emerald-50">
                {actor.fullname || `#${actor.user_id}`}
                <span className="ml-1 text-emerald-300/80">#{actor.user_id}</span>
              </div>
                <div className="mt-0.5 text-xs font-mono uppercase tracking-widest text-emerald-300/80">
                {actor.role_name
                  ? roleDisplayBi(actor.role_name, localeSafe)
                  : roleFallbackLabel(actor.kind, localeSafe) ?? '—'}
                <span className="mx-1 text-emerald-700">·</span>
                <span className="text-emerald-300/70 normal-case tracking-normal">
                  #{actor.sequence} {eventKindLabelBi(actor.kind, localeSafe)}
                </span>
                <span className="mx-1 text-emerald-700">·</span>
                <span className="text-emerald-300/70">{formatDateServer(actor.occurred_at, localeSafe)}</span>
              </div>
            </div>
          </div>
        ) : (
          <p className="text-sm text-emerald-100">
            {bi('no event recorded at this pip', 'ไม่มีเหตุการณ์ที่บันทึก', undefined, localeSafe)}
          </p>
        )}
        <ApproversList
          approvers={approvers}
          actedUsers={actedUsers}
          locale={localeSafe}
          tone="emerald"
          currentUserId={null}
          title={bi('Approvers', 'ผู้อนุมัติ', undefined, localeSafe)}
        />
        {canReCall && (
          <form action={approveWaybillAction} className="mt-2">
            <input type="hidden" name="waybillId" value={waybillId} />
            <input type="hidden" name="stage" value={pipKey} />
            <button
              type="submit"
              data-testid={`panel-recall-${pipKey}`}
              className="inline-flex items-center gap-1.5 rounded-md border border-amber-500/50 bg-amber-950/40 px-3 py-1.5 text-xs font-mono font-bold text-amber-200 transition hover:bg-amber-500/20"
              title={bi(
                'Pull the waybill back to this stage for re-review',
                'ดึงเรื่องกลับมาที่ขั้นนี้',
                undefined,
                localeSafe,
              )}
            >
              <span aria-hidden>↩</span>
              <span>{bi('Re-call to here', 'ดึงกลับมาที่นี่', undefined, localeSafe)}</span>
            </button>
          </form>
        )}
      </section>
    );
  }

  return (
    <ZoneSection
      icon={<span aria-hidden>🪜</span>}
      label={bi('Next', 'ขั้นตอนถัดไป', undefined, localeSafe)}
      tone="amber"
    >
      <ApproversList
        approvers={approvers}
        locale={localeSafe}
        tone="cyan"
        currentUserId={null}
        title={bi('Approvers', 'ผู้อนุมัติ', undefined, localeSafe)}
      />
    </ZoneSection>
  );
}
