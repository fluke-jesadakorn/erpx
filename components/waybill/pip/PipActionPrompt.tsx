import React from 'react';
import Link from 'next/link';
import type { PipState, WaybillStagePip } from '@/waybill/derive';
import { stageRoleLabel, nextStageOf } from '@/waybill/derive';
import { stageLabel as pipStageLabel } from '@/waybill/labels';
import type { WaybillEventRow } from '@/waybill/events';
import type { ApproverRow } from '@/waybill/queries';
import type { VisionModel } from '@/ai/loadVisionModels';
import type { SecondaryLocale } from '@/server/locale';
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
import { T } from '@/components/i18n/TServer';

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
  domain?: import('@/waybill/derive').WaybillDomain;
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
          ✗ <T id="waybill.timeline.rejectedBy" locale={localeSafe} />
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
            <T id="waybill.timeline.noRejection" locale={localeSafe} />
          </p>
        )}
        <ApproversList
          approvers={approvers}
          locale={localeSafe}
          tone="rose"
          currentUserId={null}
          title={<T id="waybill.pip.originalApprovers" locale={localeSafe} /> as unknown as string}
        />
        {isSubmitter && (
          <form action={resubmitWaybillAction} className="rounded-2xl border border-amber-500/50 bg-amber-950/30 p-5">
            <input type="hidden" name="waybillId" value={waybillId} />
            <p className="text-sm text-amber-100">
              <T id="waybill.pip.resubmitPrompt" locale={localeSafe} />
            </p>
            <button
              type="submit"
              className="mt-4 rounded-xl bg-amber-400 px-5 py-3 text-base font-bold text-slate-950 shadow-lg shadow-amber-500/30 transition hover:bg-amber-300"
            >
              ↩ <T id="waybill.pip.resubmit" locale={localeSafe} />
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
            🔒 <T id="waybill.timeline.finalAuthorization" locale={localeSafe} />
            <span className="ml-2 text-slate-400">
              <T id="waybill.timeline.currentStage" locale={localeSafe} />
            </span>
          </span>
          <span className="rounded-md border border-fuchsia-400/40 bg-fuchsia-500/15 px-2 py-0.5 text-fuchsia-200">
            <T id="waybill.timeline.approveGlPost" locale={localeSafe} />
          </span>
        </div>
        {canAttach && <AttachmentUpload waybillId={waybillId} stage={pipKey} />}
        {showFinalRejectForm && (
          <form action={finalRejectWaybillAction} className="rounded-2xl border border-rose-500/50 bg-rose-950/30 p-5">
            <input type="hidden" name="waybillId" value={waybillId} />
            <div className="mb-3 inline-flex items-center gap-2 rounded-md border border-rose-400/40 bg-rose-500/10 px-2 py-1 text-xs font-mono uppercase tracking-widest text-rose-200">
              🔒 <T id="waybill.pip.finalRejectNoGlPost" locale={localeSafe} />
            </div>
            <label className="block text-sm font-medium text-rose-200">
              <T id="waybill.actions.reasonMin5" locale={localeSafe} />
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
                ✗ <T id="waybill.pip.confirmFinalReject" locale={localeSafe} />
              </button>
              <a
                href={`/waybill/${waybillId}`}
                className="rounded-xl border border-slate-700 px-5 py-3 text-base text-slate-300 hover:border-slate-500"
              >
                <T id="waybill.pip.cancel" locale={localeSafe} />
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
                <span><T id="waybill.pip.finalApprove" locale={localeSafe} /></span>
              </span>
              <span className="text-xs font-mono uppercase tracking-widest text-emerald-950/70 group-hover:text-emerald-950">
                <T id="waybill.pip.finalApprovePostsGl" locale={localeSafe} />
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
                <span><T id="waybill.pip.finalReject" locale={localeSafe} /></span>
              </span>
              <span className="text-xs font-mono uppercase tracking-widest text-rose-950/70 group-hover:text-rose-950">
                <T id="waybill.timeline.finalRejectNoGl" locale={localeSafe} />
              </span>
            </Link>
          )}
        </div>
        <ApproversList
          approvers={approvers}
          locale={localeSafe}
          tone="cyan"
          currentUserId={null}
          title={<T id="waybill.approver.finalApprovers" locale={localeSafe} /> as unknown as string}
        />
      </section>
    );
  }

  if (isCurrentStage && isDisbursed && canConfirmGl) {
    return (
      <section className="space-y-3 border-t border-emerald-500/30 pt-4">
        <div className="text-xs font-mono uppercase tracking-widest text-emerald-300">
          ✅ <T id="waybill.timeline.confirmGlRecorded" locale={localeSafe} />
          <span className="ml-2 text-slate-400">
            <T id="waybill.timeline.currentStage" locale={localeSafe} />
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
              <T id="waybill.timeline.glPostedPrompt" locale={localeSafe} />
            </p>
            <button
              type="submit"
              data-testid={`panel-gl-confirm-${waybillId}`}
              className="inline-flex w-full items-center justify-center gap-2 rounded-xl bg-gradient-to-br from-amber-400 to-amber-500 px-5 py-5 text-lg font-bold text-slate-950 shadow-lg shadow-amber-500/40 transition hover:from-amber-300 hover:to-amber-400 hover:shadow-amber-500/50"
            >
              <span aria-hidden className="text-2xl">✓</span>
              <span><T id="waybill.timeline.confirmGlRecorded" locale={localeSafe} /></span>
            </button>
          </form>
        ) : (
          <p className="text-sm font-mono text-slate-400">
            <T id="waybill.timeline.waitingGlPost" locale={localeSafe} />
          </p>
        )}
        <ApproversList
          approvers={approvers}
          locale={localeSafe}
          tone="emerald"
          currentUserId={null}
          title={<T id="waybill.approver.confirmerApprovers" locale={localeSafe} /> as unknown as string}
        />
      </section>
    );
  }

  if (isCurrentStage && isAwaitingDisbursement && canSettle) {
    return (
      <section className="space-y-3 border-t border-cyan-500/30 pt-4">
        <div className="text-xs font-mono uppercase tracking-widest text-cyan-300">
          💸 <T id="waybill.timeline.disbursementStep" locale={localeSafe} />
          <span className="ml-2 text-slate-400">
            <T id="waybill.timeline.currentStage" locale={localeSafe} />
          </span>
        </div>
        {originId != null ? (
          <SettleForm waybillId={waybillId} expenseId={originId} visionModels={visionModels} />
        ) : (
          <p className="text-sm font-mono text-slate-400">
            no expenseId available to settle
          </p>
        )}
        <ApproversList
          approvers={approvers}
          locale={localeSafe}
          tone="cyan"
          currentUserId={null}
          title={<T id="waybill.approver.disbursers" locale={localeSafe} /> as unknown as string}
        />
      </section>
    );
  }

  if (isCurrentStage && canAct) {
    const roleText = stageRoleLabel(pipKey, localeSafe);
    const showRejectForm = action === 'reject' && actionStage === pipKey;
    const nextStage = nextStageOf(domain, pipKey);
    const nextStageEn = nextStage ? pipStageLabel(nextStage, domain, 'en') : null;
    const eligibleNames = approverNames(approvers);
    return (
      <section className="space-y-7 rounded-2xl border border-cyan-500/30 bg-gradient-to-br from-cyan-950/35 via-slate-950/40 to-slate-950/30 p-6 shadow-[0_0_0_1px_rgba(6,182,212,0.12),0_8px_28px_-10px_rgba(6,182,212,0.3)]">
        <div className="flex flex-wrap items-center gap-2 text-xs font-mono uppercase tracking-widest">
          <span aria-hidden className="inline-flex h-5 w-5 items-center justify-center rounded-full bg-cyan-500/20 text-cyan-300">
            ⚡
          </span>
          <span className="text-cyan-200">
            <T id="waybill.timeline.actingNow" locale={localeSafe} />
          </span>
          <span className="text-slate-600">·</span>
          <span className="text-slate-400">
            <T id="waybill.timeline.currentStage" locale={localeSafe} />
          </span>
          <span className="ml-auto inline-flex items-center gap-1.5 rounded-full border border-cyan-500/40 bg-cyan-500/15 px-2 py-0.5 font-mono font-bold uppercase tracking-widest text-cyan-200">
            <span className="text-cyan-300/80">role:</span>
             <span className="text-cyan-100"><T id={roleText} locale={localeSafe} /></span>
          </span>
        </div>
        {eligibleNames.length > 0 && (
          <div className="flex flex-wrap items-center gap-2 rounded-lg border border-cyan-500/20 bg-cyan-950/20 px-3 py-2 text-sm text-cyan-100">
            <span aria-hidden className="text-cyan-300">👤</span>
            <span className="font-mono uppercase tracking-widest text-cyan-300/80">
              <T id="waybill.pip.eligibleApprovers" locale={localeSafe} />
            </span>
            <span className="text-slate-500">·</span>
            <span className="text-cyan-50">
              {eligibleNames.join(', ')}
              {approvers.length > eligibleNames.length && (
                <span className="text-slate-400">
                  {' '}
                  +{approvers.length - eligibleNames.length} more
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
              <T id="waybill.actions.reasonMin5" locale={localeSafe} />
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
                ✗ <T id="waybill.pip.confirmReject" locale={localeSafe} />
              </button>
              <a
                href={`/waybill/${waybillId}`}
                className="rounded-xl border border-slate-700 px-5 py-3 text-base text-slate-300 hover:border-slate-500"
              >
                <T id="waybill.pip.cancel" locale={localeSafe} />
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
                <span><T id="waybill.timeline.approve" locale={localeSafe} /></span>
              </span>
              {nextStageEn ? (
                <span className="inline-flex items-center gap-1 text-xs font-mono uppercase tracking-widest text-cyan-950/80 group-hover:text-cyan-950">
                  <span><T id="waybill.pip.advanceTo" locale={localeSafe} /></span>
                  <span className="text-cyan-950/90">{nextStageEn.emoji}</span>
                   <span className="text-cyan-950/90"><T id={nextStageEn.label} locale={localeSafe} /></span>
                </span>
              ) : (
                <span className="text-xs font-mono uppercase tracking-widest text-cyan-950/80 group-hover:text-cyan-950">
                  <T id="waybill.pip.closeThisStep" locale={localeSafe} />
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
              <span><T id="waybill.timeline.reject" locale={localeSafe} /></span>
            </Link>
          )}
        </div>
        <div className="mt-2 border-t border-cyan-500/15 pt-5">
          <ApproversList
            approvers={approvers}
            locale={localeSafe}
            tone="cyan"
            currentUserId={null}
            title={<T id="waybill.approver.approvers" locale={localeSafe} /> as unknown as string}
          />
        </div>
      </section>
    );
  }

  if (isPassed) {
    const actor = pickActor(actedUsers);
    const label = actor
      ? eventKindLabelBi(actor.kind, localeSafe)
      : <T id="waybill.timeline.completedBy" locale={localeSafe} />;
    const formattedDate = actor ? formatDateServer(actor.occurred_at, localeSafe) : null;
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
                  : '—'}
                <span className="mx-1 text-emerald-700">·</span>
                <span className="text-emerald-300/70 normal-case tracking-normal">
                  #{actor.sequence} {eventKindLabelBi(actor.kind, localeSafe)}
                </span>
                <span className="mx-1 text-emerald-700">·</span>
                <span className="text-emerald-300/70">{formattedDate}</span>
              </div>
            </div>
          </div>
        ) : (
          <p className="text-sm text-emerald-100">
            <T id="waybill.timeline.noEventAtPip" locale={localeSafe} />
          </p>
        )}
        <ApproversList
          approvers={approvers}
          actedUsers={actedUsers}
          locale={localeSafe}
          tone="emerald"
          currentUserId={null}
          title={<T id="waybill.approver.approvers" locale={localeSafe} /> as unknown as string}
        />
        {canReCall && (
          <form action={approveWaybillAction} className="mt-2">
            <input type="hidden" name="waybillId" value={waybillId} />
            <input type="hidden" name="stage" value={pipKey} />
            <button
              type="submit"
              data-testid={`panel-recall-${pipKey}`}
              className="inline-flex items-center gap-1.5 rounded-md border border-amber-500/50 bg-amber-950/40 px-3 py-1.5 text-xs font-mono font-bold text-amber-200 transition hover:bg-amber-500/20"
              title="Pull the waybill back to this stage for re-review"
            >
              <span aria-hidden>↩</span>
              <span><T id="waybill.pip.recallToHere" locale={localeSafe} /></span>
            </button>
          </form>
        )}
      </section>
    );
  }

  return (
    <ZoneSection
      icon={<span aria-hidden>🪜</span>}
      label={<T id="waybill.pip.actingNext" locale={localeSafe} />}
      tone="amber"
    >
      <ApproversList
        approvers={approvers}
        locale={localeSafe}
        tone="cyan"
        currentUserId={null}
        title={<T id="waybill.approver.approvers" locale={localeSafe} /> as unknown as string}
      />
    </ZoneSection>
  );
}
