import React from 'react';
import type { PipState, WaybillStagePip, WaybillDomain } from '@/waybill/derive';
import type { SecondaryLocale } from '@/server/locale';
import type { WaybillEventRow } from '@/waybill/events';
import type { WaybillAttachmentRow } from '@/waybill/attachments';
import type { ApproverRow, ExpenseArtifacts, ProcurementArtifacts } from '@/waybill/queries';
import type { VisionModel } from '@/ai/loadVisionModels';
import { type ActedUserLite } from '../ApproversList';
import { PipDocuments } from './PipDocuments';
import { PipArtifacts } from './PipArtifacts';
import { PipActionPrompt } from './PipActionPrompt';

type StepperArtifacts = ExpenseArtifacts | ProcurementArtifacts | null;

interface Props {
  waybillId: string;
  pip: WaybillStagePip;
  state: PipState;
  currentStage: string;
  events: WaybillEventRow[];
  attachments: WaybillAttachmentRow[];
  approvers: ApproverRow[];
  actedUsers: ActedUserLite[];
  artifacts: StepperArtifacts;
  actorCanSeeGlLines: boolean;
  visionModels: VisionModel[];
  isRejected: boolean;
  rejectionReason: string | null;
  rejectionActor: { user_id: number; fullname: string; role: string | null } | null;
  isSubmitter: boolean;
  action: string | null;
  actionStage: string | null;
  originId: number | null;
  domain: WaybillDomain;
  locale?: SecondaryLocale;
  actions: {
    canAct: boolean;
    canAttach: boolean;
    canSettle: boolean;
    canFinalApprove: boolean;
    canConfirmGl: boolean;
    canReCall: boolean;
    canPostAccrual: boolean;
    canConfirmAccrual: boolean;
    canPostSettlement: boolean;
    canConfirmSettlement: boolean;
  };
}

export function PipCard({
  waybillId,
  pip,
  state,
  currentStage,
  events,
  attachments: _attachments,
  approvers,
  actedUsers,
  artifacts,
  actorCanSeeGlLines,
  visionModels,
  isRejected,
  rejectionReason,
  rejectionActor,
  isSubmitter,
  action,
  actionStage,
  originId,
  domain,
  locale,
  actions,
}: Props) {
  const isCurrentStage = currentStage === pip.key;
  const isFinalApproval = pip.key === 'final_authorization';
  const isDisbursed = pip.key === 'disbursed';
  const isAwaitingDisbursement = pip.key === 'awaiting_disbursement';
  const isRejection = state === 'rejected' || isRejected;
  const isPassed = state === 'passed';

  const canAct = isCurrentStage && actions.canAct && !isRejection;
  const canAttach = isCurrentStage && actions.canAttach && !isRejection;
  const canSettle = isCurrentStage && isAwaitingDisbursement && actions.canSettle && !isRejection;
  const canFinalApprove = isCurrentStage && isFinalApproval && actions.canFinalApprove && !isRejection;
  const canConfirmGl = isCurrentStage && isDisbursed && actions.canConfirmGl && !isRejection;

  return (
    <article className="min-w-0 space-y-6">
      {isCurrentStage && (
        <PipDocuments waybillId={waybillId} pipKey={pip.key} locale={locale} />
      )}

      <PipArtifacts
        waybillId={waybillId}
        pip={pip}
        artifacts={artifacts}
        actorCanSeeGlLines={actorCanSeeGlLines}
        originId={originId}
        isRejection={isRejection}
        isCurrentStage={isCurrentStage}
        locale={locale}
        actions={{
          canFinalApprove,
          canConfirmGl,
          canSettle,
          canPostAccrual: actions.canPostAccrual,
          canConfirmAccrual: actions.canConfirmAccrual,
          canPostSettlement: actions.canPostSettlement,
          canConfirmSettlement: actions.canConfirmSettlement,
        }}
      />

      <PipActionPrompt
        waybillId={waybillId}
        pip={pip}
        pipKey={pip.key}
        state={state}
        isCurrentStage={isCurrentStage}
        isRejection={isRejection}
        isPassed={isPassed}
        isFinalApproval={isFinalApproval}
        isDisbursed={isDisbursed}
        isAwaitingDisbursement={isAwaitingDisbursement}
        canAct={canAct}
        canAttach={canAttach}
        canSettle={canSettle}
        canFinalApprove={canFinalApprove}
        canConfirmGl={canConfirmGl}
        canReCall={actions.canReCall}
        hasGlConfirmed={false}
        originId={originId}
        approvers={approvers}
        rejectionReason={rejectionReason}
        rejectionActor={rejectionActor}
        visionModels={visionModels}
        events={events}
        actedUsers={actedUsers}
        action={action}
        actionStage={actionStage}
        isSubmitter={isSubmitter}
        domain={domain}
        locale={locale}
      />
    </article>
  );
}
