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

const FINAL_APPROVAL_BG = 'border-accent bg-accent-strong';
const FINAL_APPROVAL_TEXT = 'text-accent-strong';

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
      <section className="space-y-3 border-t border-critical pt-4">
        <div className="text-xs font-mono uppercase tracking-widest text-critical-strong">
          ✗ <T id="waybill.timeline.rejectedBy" locale={localeSafe} />
        </div>
        {rejectionActor ? (
          <div className="rounded-lg border border-critical bg-critical-soft p-3 text-sm text-critical-strong">
            <span className="font-mono">
              {rejectionActor.fullname}
              <span className="ml-1 text-critical">#{rejectionActor.user_id}</span>
            </span>
            {rejectionActor.role && (
              <span className="ml-2 text-critical">
                ({roleDisplayBi(rejectionActor.role, localeSafe)})
              </span>
            )}
            {rejectionReason && (
              <p className="mt-2 italic text-critical-strong">&ldquo;{rejectionReason}&rdquo;</p>
            )}
          </div>
        ) : (
          <p className="text-sm text-critical-strong">
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
          <form action={resubmitWaybillAction} className="rounded-md border border-caution bg-caution-soft p-5">
            <input type="hidden" name="waybillId" value={waybillId} />
            <p className="text-sm text-caution-strong">
              <T id="waybill.pip.resubmitPrompt" locale={localeSafe} />
            </p>
            <button
              type="submit"
              className="mt-4 rounded-md bg-caution px-5 py-3 text-base font-bold text-paper shadow-lg shadow-caution transition hover:bg-caution"
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
            <span className="ml-2 text-ink-2">
              <T id="waybill.timeline.currentStage" locale={localeSafe} />
            </span>
          </span>
          <span className="rounded-md border border-accent bg-accent px-2 py-0.5 text-accent-strong">
            <T id="waybill.timeline.approveGlPost" locale={localeSafe} />
          </span>
        </div>
        {canAttach && <AttachmentUpload waybillId={waybillId} stage={pipKey} />}
        {showFinalRejectForm && (
          <form action={finalRejectWaybillAction} className="rounded-md border border-critical bg-critical-soft p-5">
            <input type="hidden" name="waybillId" value={waybillId} />
            <div className="mb-3 inline-flex items-center gap-2 rounded-md border border-critical bg-critical px-2 py-1 text-xs font-mono uppercase tracking-widest text-critical-strong">
              🔒 <T id="waybill.pip.finalRejectNoGlPost" locale={localeSafe} />
            </div>
            <label className="block text-sm font-medium text-critical-strong">
              <T id="waybill.actions.reasonMin5" locale={localeSafe} />
              <textarea
                name="reason"
                required
                minLength={5}
                className="mt-2 block w-full rounded-lg bg-paper p-3 text-sm text-ink ring-1 ring-critical focus:outline-none focus:ring-2 focus:ring-critical"
                rows={3}
              />
            </label>
            <div className="mt-4 flex gap-3">
              <button
                type="submit"
                className="rounded-md bg-critical px-5 py-3 text-base font-bold text-paper shadow-lg shadow-critical transition hover:bg-critical"
              >
                ✗ <T id="waybill.pip.confirmFinalReject" locale={localeSafe} />
              </button>
              <a
                href={`/waybill/${waybillId}`}
                className="rounded-md border border-rule px-5 py-3 text-base text-ink-2 hover:border-rule"
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
              className="group inline-flex w-full flex-col items-center justify-center gap-1 rounded-md bg-positive px-5 py-5 text-lg font-bold text-paper shadow-xl shadow-positive transition hover:bg-positive-strong hover:shadow-positive-strong"
            >
              <span className="flex items-center gap-2">
                <span aria-hidden className="text-2xl">✓</span>
                <span><T id="waybill.pip.finalApprove" locale={localeSafe} /></span>
              </span>
              <span className="text-xs font-mono uppercase tracking-widest text-positive-strong group-hover:text-positive-strong">
                <T id="waybill.pip.finalApprovePostsGl" locale={localeSafe} />
              </span>
            </button>
          </form>
          {!showFinalRejectForm && (
            <Link
              href={`/waybill/${waybillId}?action=final-reject&stage=${pipKey}`}
              data-testid={`panel-final-reject-${waybillId}`}
              className="group inline-flex flex-col items-center justify-center gap-1 rounded-md bg-critical px-5 py-5 text-lg font-bold text-paper shadow-xl shadow-critical transition hover:bg-critical-strong hover:shadow-critical-strong"
            >
              <span className="flex items-center gap-2">
                <span aria-hidden className="text-2xl">✗</span>
                <span><T id="waybill.pip.finalReject" locale={localeSafe} /></span>
              </span>
              <span className="text-xs font-mono uppercase tracking-widest text-critical-strong group-hover:text-critical-strong">
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
      <section className="space-y-3 border-t border-positive pt-4">
        <div className="text-xs font-mono uppercase tracking-widest text-positive">
          ✅ <T id="waybill.timeline.confirmGlRecorded" locale={localeSafe} />
          <span className="ml-2 text-ink-2">
            <T id="waybill.timeline.currentStage" locale={localeSafe} />
          </span>
        </div>
        {originId != null ? (
          <form
            action={confirmGlRecordedAction}
            className="rounded-md border border-caution bg-caution-soft p-4 text-sm text-caution-strong"
          >
            <input type="hidden" name="waybillId" value={waybillId} />
            <input type="hidden" name="expenseId" value={originId} />
            <p className="mb-3 font-mono">
              <T id="waybill.timeline.glPostedPrompt" locale={localeSafe} />
            </p>
            <button
              type="submit"
              data-testid={`panel-gl-confirm-${waybillId}`}
              className="inline-flex w-full items-center justify-center gap-2 rounded-md bg-caution px-5 py-5 text-lg font-bold text-paper shadow-lg shadow-caution transition hover:bg-caution-strong hover:shadow-caution-strong"
            >
              <span aria-hidden className="text-2xl">✓</span>
              <span><T id="waybill.timeline.confirmGlRecorded" locale={localeSafe} /></span>
            </button>
          </form>
        ) : (
          <p className="text-sm font-mono text-ink-2">
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
      <section className="space-y-3 border-t border-info pt-4">
        <div className="text-xs font-mono uppercase tracking-widest text-info">
          💸 <T id="waybill.timeline.disbursementStep" locale={localeSafe} />
          <span className="ml-2 text-ink-2">
            <T id="waybill.timeline.currentStage" locale={localeSafe} />
          </span>
        </div>
        {originId != null ? (
          <SettleForm waybillId={waybillId} expenseId={originId} visionModels={visionModels} />
        ) : (
          <p className="text-sm font-mono text-ink-2">
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
      <section className="space-y-7 rounded-md border border-info bg-info-soft p-6">
        <div className="flex flex-wrap items-center gap-2 text-xs font-mono uppercase tracking-widest">
          <span aria-hidden className="inline-flex h-5 w-5 items-center justify-center rounded-full bg-info text-paper">
            ⚡
          </span>
          <span className="text-info-strong">
            <T id="waybill.timeline.actingNow" locale={localeSafe} />
          </span>
          <span className="text-mute">·</span>
          <span className="text-ink-2">
            <T id="waybill.timeline.currentStage" locale={localeSafe} />
          </span>
          <span className="ml-auto inline-flex items-center gap-1.5 rounded-full border border-info bg-info px-2 py-0.5 font-mono font-bold uppercase tracking-widest text-info-strong">
            <span className="text-info">role:</span>
             <span className="text-info-strong"><T id={roleText} locale={localeSafe} /></span>
          </span>
        </div>
        {eligibleNames.length > 0 && (
          <div className="flex flex-wrap items-center gap-2 rounded-lg border border-info bg-info-strong px-3 py-2 text-sm text-info-strong">
            <span aria-hidden className="text-info">👤</span>
            <span className="font-mono uppercase tracking-widest text-info">
              <T id="waybill.pip.eligibleApprovers" locale={localeSafe} />
            </span>
            <span className="text-mute">·</span>
            <span className="text-info-strong">
              {eligibleNames.join(', ')}
              {approvers.length > eligibleNames.length && (
                <span className="text-ink-2">
                  {' '}
                  +{approvers.length - eligibleNames.length} more
                </span>
              )}
            </span>
          </div>
        )}
        {canAttach && <AttachmentUpload waybillId={waybillId} stage={pipKey} />}
        {showRejectForm && (
          <form action={rejectWaybillAction} className="rounded-md border border-critical bg-critical-soft p-5">
            <input type="hidden" name="waybillId" value={waybillId} />
            <input type="hidden" name="stage" value={pipKey} />
            <label className="block text-sm font-medium text-critical-strong">
              <T id="waybill.actions.reasonMin5" locale={localeSafe} />
              <textarea
                name="reason"
                required
                minLength={5}
                className="mt-2 block w-full rounded-lg bg-paper p-3 text-sm text-ink ring-1 ring-critical focus:outline-none focus:ring-2 focus:ring-critical"
                rows={3}
              />
            </label>
            <div className="mt-4 flex gap-3">
              <button
                type="submit"
                className="rounded-md bg-critical px-5 py-3 text-base font-bold text-paper shadow-lg shadow-critical transition hover:bg-critical"
              >
                ✗ <T id="waybill.pip.confirmReject" locale={localeSafe} />
              </button>
              <a
                href={`/waybill/${waybillId}`}
                className="rounded-md border border-rule px-5 py-3 text-base text-ink-2 hover:border-rule"
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
              className="group relative inline-flex w-full flex-col items-center justify-center gap-1.5 overflow-hidden rounded-md bg-info px-5 py-5 text-lg font-bold text-ink shadow-popover transition hover:bg-info-strong"
            >
              <span className="flex items-center gap-2">
                <span aria-hidden className="text-2xl leading-none">✓</span>
                <span><T id="waybill.timeline.approve" locale={localeSafe} /></span>
              </span>
              {nextStageEn ? (
                <span className="inline-flex items-center gap-1 text-xs font-mono uppercase tracking-widest text-info-strong group-hover:text-info-strong">
                  <span><T id="waybill.pip.advanceTo" locale={localeSafe} /></span>
                  <span className="text-info-strong">{nextStageEn.emoji}</span>
                   <span className="text-info-strong"><T id={nextStageEn.label} locale={localeSafe} /></span>
                </span>
              ) : (
                <span className="text-xs font-mono uppercase tracking-widest text-info-strong group-hover:text-info-strong">
                  <T id="waybill.pip.closeThisStep" locale={localeSafe} />
                </span>
              )}
            </button>
          </form>
          {!showRejectForm && (
            <Link
              href={`/waybill/${waybillId}?action=reject&stage=${pipKey}`}
              data-testid={`panel-reject-${pipKey}`}
              className="group inline-flex items-center justify-center gap-2 rounded-md border border-rule/80 bg-paper-2/60 px-4 py-4 text-sm font-bold text-ink-2 shadow-inner transition hover:border-critical hover:bg-critical-strong hover:text-critical-strong"
            >
              <span aria-hidden className="text-lg leading-none">✗</span>
              <span><T id="waybill.timeline.reject" locale={localeSafe} /></span>
            </Link>
          )}
        </div>
        <div className="mt-2 border-t border-info pt-5">
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
      <section className="space-y-3 border-t border-positive pt-4">
        <div className="text-xs font-mono uppercase tracking-widest text-positive-strong">
          ✓ {label}
        </div>
        {actor ? (
          <div className="flex items-center gap-3 rounded-lg border border-positive bg-positive-soft p-3 text-sm text-positive-strong">
            <span
              aria-hidden
              className="inline-flex h-9 w-9 shrink-0 items-center justify-center rounded-full border border-positive bg-positive text-xs font-bold uppercase text-positive-strong"
            >
              {initialsOf(actor.fullname)}
            </span>
            <div className="min-w-0 flex-1">
              <div className="truncate font-semibold text-positive-strong">
                {actor.fullname || `#${actor.user_id}`}
                <span className="ml-1 text-positive">#{actor.user_id}</span>
              </div>
                <div className="mt-0.5 text-xs font-mono uppercase tracking-widest text-positive">
                {actor.role_name
                  ? roleDisplayBi(actor.role_name, localeSafe)
                  : '—'}
                <span className="mx-1 text-positive-strong">·</span>
                <span className="text-positive normal-case tracking-normal">
                  #{actor.sequence} {eventKindLabelBi(actor.kind, localeSafe)}
                </span>
                <span className="mx-1 text-positive-strong">·</span>
                <span className="text-positive">{formattedDate}</span>
              </div>
            </div>
          </div>
        ) : (
          <p className="text-sm text-positive-strong">
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
              className="inline-flex items-center gap-1.5 rounded-md border border-caution bg-caution-strong px-3 py-1.5 text-xs font-mono font-bold text-caution-strong transition hover:bg-caution"
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
