import React from 'react';
import type { SecondaryLocale } from '@/server/locale';
import type {
  ApproverRow,
  ActedUserEntry,
  ExpenseArtifacts,
  ProcurementArtifacts,
  WaybillRow,
} from '@/waybill/queries';
import type { WaybillEventRow } from '@/waybill/events';
import type { WaybillAttachmentRow } from '@/waybill/attachments';
import type { VisionModel } from '@/ai/loadVisionModels';
import type { ExpensePaymentPreview } from '@/finance/expenseDocument';
import {
  computePipState,
  domainForOrigin,
  findPip,
  pipIndex,
  type WaybillDomain,
} from '@/waybill/derive';
import { claimExpenseStageAction } from '@/app/actions/waybill';
import { T } from '@/components/i18n/T';
import { Badge, Panel } from '@/components/ui';
import { ApproversList } from './ApproversList';
import { AiRecommendChip } from './AiRecommendChip';
import { WaybillReviewHint } from './WaybillReviewHint';
import { PipActionPrompt } from './pip/PipActionPrompt';
import { PipArtifacts } from './pip/PipArtifacts';
import { PipDocuments } from './pip/PipDocuments';
import type { ActionsBag, FlagsBag, RejectionBag, UiBag } from './WaybillStepCards';

type TaskArtifacts = ExpenseArtifacts | ProcurementArtifacts | null;

interface Props {
  wb: WaybillRow;
  waybillId: string;
  currentStage: string;
  status: string;
  events: WaybillEventRow[];
  attachments: WaybillAttachmentRow[];
  approversByStage: Record<string, ApproverRow[]>;
  actedUsersByStage: Record<string, ActedUserEntry[]>;
  artifacts?: TaskArtifacts;
  actorCanSeeGlLines: boolean;
  originId: number | null;
  visionModels: VisionModel[];
  actions: ActionsBag;
  flags: FlagsBag;
  rejection: RejectionBag;
  ui: UiBag;
  claim?: {
    claimedBy: number | null;
    claimedByName: string | null;
    isMine: boolean;
    canClaim: boolean;
  } | null;
  currentUserId: number | null;
  action?: string | null;
  domain?: WaybillDomain;
  locale?: SecondaryLocale;
  vendorName?: string | null;
  amount?: string | number | null;
  payment?: ExpensePaymentPreview | null;
}

export function WaybillTaskPanel({
  wb,
  waybillId,
  currentStage,
  status,
  events,
  attachments: _attachments,
  approversByStage,
  actedUsersByStage,
  artifacts = null,
  actorCanSeeGlLines,
  originId,
  visionModels,
  actions,
  flags,
  rejection,
  ui,
  claim,
  currentUserId,
  action = null,
  domain = domainForOrigin(wb.origin),
  locale = 'th',
  vendorName,
  amount,
  payment,
}: Props) {
  const pip = findPip(domain, currentStage);
  const index = pipIndex(domain, currentStage);
  const approvers = approversByStage[currentStage] ?? [];
  const actedUsers = actedUsersByStage[currentStage] ?? [];
  const isRejected = status === 'rejected' || currentStage === 'rejected';
  const effectiveCanAct = actions.canAct && (!claim || claim.isMine || !claim.claimedBy);

  if (!pip || index < 0) {
    return (
      <div id="waybill-task">
        <Panel tone="floating" padding="md" className="space-y-3">
          <div className="flex items-center gap-2">
            <Badge tone="neutral" size="sm"><T id="waybill.decisions.currentStage" /></Badge>
            <span className="font-mono text-sm text-ink">{currentStage}</span>
          </div>
          <p className="text-sm text-ink-2"><T id="waybill.pip.locked" /></p>
        </Panel>
      </div>
    );
  }

  const state = computePipState(pip, index, index, currentStage, status);
  const actionProps = {
    ...actions,
    canAct: effectiveCanAct,
    canSettle: actions.canSettle && effectiveCanAct,
    canFinalApprove: actions.canFinalApprove && effectiveCanAct,
    canConfirmGl: actions.canConfirmGl && effectiveCanAct,
  };
  const showActionPrompt = isRejected
    || status === 'completed'
    || state === 'passed'
    || actionProps.canAct
    || actionProps.canFinalApprove
    || actionProps.canSettle
    || actionProps.canConfirmGl;

  return (
    <div id="waybill-task">
      <Panel tone="floating" padding="none" className="overflow-hidden">
      <header className="border-b border-rule/80 bg-paper-2/70 px-4 py-4">
        <div className="space-y-2">
          <div className="min-w-0">
            <p className="text-[11px] font-mono uppercase tracking-[0.16em] text-info">
              <T id="waybill.decisions.currentStage" variant="compact" />
            </p>
            <div className="mt-1 flex flex-wrap items-center gap-2">
              <h2 className="text-lg font-semibold leading-tight text-ink">
                <T
                  id={pip.label}
                  variant="stacked"
                  primaryClassName="block text-lg font-semibold leading-tight text-ink"
                  secondaryClassName="mt-0.5 block text-xs font-normal leading-snug text-mute"
                />
              </h2>
              <Badge tone={isRejected ? 'critical' : state === 'passed' ? 'positive' : 'accent'} size="sm">
                <T id={isRejected ? 'waybill.pip.stop' : state === 'passed' ? 'waybill.pip.done' : effectiveCanAct ? 'waybill.pip.yourTurn' : 'waybill.pip.locked'} />
              </Badge>
            </div>
            <p className="mt-2 text-sm leading-relaxed text-ink-2">
              <T
                id={pip.description}
                variant="stacked"
                primaryClassName="block text-sm font-normal leading-relaxed text-ink-2"
                secondaryClassName="mt-1 block text-xs font-normal leading-relaxed text-mute"
              />
            </p>
          </div>
        </div>
        {!showActionPrompt && approvers.length > 0 && (
          <div className="mt-4 border-t border-rule/60 pt-3">
            <ApproversList
              approvers={approvers}
              actedUsers={actedUsers}
              currentUserId={currentUserId}
              locale={locale}
              title={<T id="waybill.approver.approvers" variant="compact" /> as unknown as string}
            />
          </div>
        )}
      </header>

      <div className="space-y-4 p-4">
        {claim?.canClaim && (
          <form action={claimExpenseStageAction} className="rounded-lg border border-positive/50 bg-positive-soft p-3">
            <input type="hidden" name="waybillId" value={waybillId} />
            <p className="text-sm font-semibold text-positive">
              <T id="waybill.claim.title" />
            </p>
            <p className="mt-1 text-xs text-ink-2"><T id="waybill.claim.hint" /></p>
            <button type="submit" className="mt-3 inline-flex min-h-11 w-full items-center justify-center rounded-lg bg-positive px-4 py-2.5 text-sm font-semibold text-paper transition hover:bg-positive-strong">
              <T id="waybill.claim.action" />
            </button>
          </form>
        )}

        {claim?.claimedBy && !claim.isMine && (
          <div className="rounded-lg border border-caution/50 bg-caution-soft p-3 text-sm text-caution-strong">
            <p className="font-semibold"><T id="waybill.claim.claimed" /></p>
            <p className="mt-1 text-xs"><T id="waybill.claim.claimedBy" values={{ name: claim.claimedByName ?? `user #${claim.claimedBy}` }} /></p>
          </div>
        )}

        {showActionPrompt && (
          <PipActionPrompt
            waybillId={waybillId}
            pip={pip}
            pipKey={pip.key}
            state={state}
            isCurrentStage
            isRejection={isRejected}
            isPassed={status === 'completed' || state === 'passed'}
            isFinalApproval={pip.key === 'accounting_approval' || pip.key === 'final_authorization'}
            isDisbursed={pip.key === 'settlement' || pip.key === 'disbursed'}
            isAwaitingDisbursement={pip.key === 'payment' || pip.key === 'awaiting_disbursement'}
            canAct={actionProps.canAct}
            canAttach={actionProps.canAttach}
            canSettle={actionProps.canSettle}
            canFinalApprove={actionProps.canFinalApprove}
            canConfirmGl={actionProps.canConfirmGl}
            canReCall={actionProps.canReCall}
            hasGlConfirmed={false}
            originId={originId}
            approvers={approvers}
            rejectionReason={rejection.reason}
            rejectionActor={rejection.actor}
            visionModels={visionModels}
            events={events}
            actedUsers={actedUsers}
            action={action}
            actionStage={ui.actionStage}
            isSubmitter={flags.isSubmitter}
            domain={domain}
            locale={locale}
            payment={payment}
          />
        )}

        {pip.key !== currentStage || isRejected ? null : (
          <PipDocuments waybillId={waybillId} pipKey={pip.key} locale={locale} />
        )}

        <PipArtifacts
          waybillId={waybillId}
          pip={pip}
          artifacts={artifacts}
          actorCanSeeGlLines={actorCanSeeGlLines}
          originId={originId}
          isRejection={isRejected}
          isCurrentStage
          locale={locale}
          actions={{
            canFinalApprove: actionProps.canFinalApprove,
            canConfirmGl: actionProps.canConfirmGl,
            canSettle: actionProps.canSettle,
            canPostAccrual: actionProps.canPostAccrual,
            canConfirmAccrual: actionProps.canConfirmAccrual,
            canPostSettlement: actionProps.canPostSettlement,
            canConfirmSettlement: actionProps.canConfirmSettlement,
          }}
        />

        <details className="group overflow-hidden rounded-xl border border-accent/30 bg-accent-soft/20">
          <summary className="flex cursor-pointer list-none items-center gap-3 px-4 py-3 [&::-webkit-details-marker]:hidden">
            <span aria-hidden className="grid h-8 w-8 shrink-0 place-items-center rounded-lg bg-accent-soft text-accent">✦</span>
            <span className="min-w-0 flex-1">
              <T
                id="waybill.ai.recommend"
                variant="stacked"
                primaryClassName="block text-sm font-semibold text-ink"
                secondaryClassName="mt-0.5 block text-xs font-normal text-mute"
              />
            </span>
            <span className="text-sm text-mute transition group-open:rotate-180" aria-hidden>⌄</span>
          </summary>
          <div className="space-y-4 border-t border-accent/20 p-4">
            <AiRecommendChip
              waybillId={waybillId}
              amount={amount ?? wb.total_amount}
              currentStage={currentStage}
              vendorName={vendorName ?? wb.vendor_name}
            />
            <div className="grid gap-3">
              <WaybillReviewHint waybillId={waybillId} lang={locale} stage="hod" label={<T id="waybill.review.hod" />} />
              <WaybillReviewHint waybillId={waybillId} lang={locale} stage="am" label={<T id="waybill.review.am" />} />
            </div>
          </div>
        </details>
      </div>
      </Panel>
    </div>
  );
}

export default WaybillTaskPanel;
