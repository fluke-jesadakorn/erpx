import React from 'react';
import type { WaybillStagePip } from '@/waybill/derive';
import type { SecondaryLocale } from '@/server/locale';
import type { ExpenseArtifacts, ProcurementArtifacts } from '@/waybill/queries';
import {
  confirmGlRecordedAction,
  finalApproveWaybillAction,
} from '@/app/actions/waybill';
import {
  postProcurementAccrualAction,
  postProcurementSettlementAction,
  confirmProcurementGlAction,
} from '@/app/actions/procurement';
import { ZoneSection } from '../ZoneSection';
import { PipArtifactPanel } from '../PipArtifactPanel';
import { T } from '@/components/i18n/TServer';

type StepperArtifacts = ExpenseArtifacts | ProcurementArtifacts | null;

interface Props {
  waybillId: string;
  pip: WaybillStagePip;
  artifacts: StepperArtifacts;
  actorCanSeeGlLines: boolean;
  originId: number | null;
  isRejection: boolean;
  isCurrentStage: boolean;
  locale?: SecondaryLocale;
  actions: {
    canFinalApprove: boolean;
    canConfirmGl: boolean;
    canSettle: boolean;
    canPostAccrual: boolean;
    canConfirmAccrual: boolean;
    canPostSettlement: boolean;
    canConfirmSettlement: boolean;
  };
}

export function PipArtifacts({
  waybillId,
  pip,
  artifacts,
  actorCanSeeGlLines,
  originId,
  isRejection,
  isCurrentStage,
  locale,
  actions,
}: Props) {
  const localeSafe: SecondaryLocale = locale ?? 'th';
  if (!artifacts || isRejection) return null;

  const isProcurementArtifacts =
    artifacts != null && (artifacts as ProcurementArtifacts).glAccrual !== undefined;
  const cards: React.ReactNode[] = [];

  if (pip.key === 'submission' && isProcurementArtifacts) {
    const a = artifacts as ProcurementArtifacts;
    if (a.pr) {
      cards.push(
        <PipArtifactPanel
          key="pr"
          kind="pr"
          waybillId={waybillId}
          artifact={{
            id: a.pr.pr_number,
            status: a.pr.status,
            href: `/pr/${a.pr.id}`,
            finalizedAt: a.pr.created_at,
            finalizedByName: a.pr.requester_name ?? null,
          }}
          canApprove={false}
          approveLabel={<T id="waybill.pip.openPr" locale={localeSafe} /> as unknown as string}
          actorCanSeeLines={actorCanSeeGlLines}
          locale={localeSafe}
        />,
      );
    }
  }

  if (pip.key === 'accounting_authorization') {
    if (isProcurementArtifacts) {
      const a = artifacts as ProcurementArtifacts;
      if (a.po) {
        cards.push(
          <PipArtifactPanel
            key="po"
            kind="po"
            waybillId={waybillId}
            artifact={{
              id: a.po.po_number,
              status: a.po.status,
              href: `/po/${a.po.id}`,
              finalizedAt: a.po.issued_at,
              finalizedByName: a.po.issuer_name ?? null,
            }}
            canApprove={false}
            approveLabel={<T id="waybill.pip.openPo" locale={localeSafe} /> as unknown as string}
            actorCanSeeLines={actorCanSeeGlLines}
            locale={localeSafe}
          />,
        );
      }
      if (a.glAccrual) {
        cards.push(
          <PipArtifactPanel
            key="gl-accrual"
            kind="gl-accrual"
            waybillId={waybillId}
            artifact={{
              id: `JE #${a.glAccrual.id}`,
              status: a.glAccrual.finalized_at ? 'posted' : 'draft',
              href: null,
              finalizedAt: a.glAccrual.finalized_at,
              finalizedByName: a.glAccrual.finalized_by_name ?? null,
              lines: a.glAccrual.lines,
            }}
            canApprove={actions.canPostAccrual && !a.glAccrual.finalized_at}
            approveAction={postProcurementAccrualAction}
            approveHiddenInputs={{ journalId: String(a.glAccrual.id), stage: 'accounting_authorization' }}
            approveLabel={<T id="waybill.pip.postAccrual" locale={localeSafe} /> as unknown as string}
            actorCanSeeLines={actorCanSeeGlLines}
            locale={localeSafe}
          />,
        );
      }
    } else {
      const a = artifacts as ExpenseArtifacts;
      if (a.po) {
        cards.push(
          <PipArtifactPanel
            key="po"
            kind="po"
            waybillId={waybillId}
            artifact={{
              id: a.po.po_number,
              status: a.po.status,
              href: `/po/${a.po.id}`,
              finalizedAt: a.po.issued_at,
              finalizedByName: a.po.issuer_name ?? null,
            }}
            canApprove={actions.canFinalApprove && isCurrentStage}
            approveAction={finalApproveWaybillAction}
            approveLabel={<T id="waybill.pip.approvePostPo" locale={localeSafe} /> as unknown as string}
            actorCanSeeLines={actorCanSeeGlLines}
            locale={localeSafe}
          />,
        );
      }
      if (a.gl) {
        cards.push(
          <PipArtifactPanel
            key="gl-accrual"
            kind="gl-accrual"
            waybillId={waybillId}
            artifact={{
              id: `JE #${a.gl.id}`,
              status: a.gl.finalized_at ? 'posted' : 'issued',
              href: null,
              finalizedAt: a.gl.finalized_at,
              finalizedByName: a.gl.finalized_by_name ?? null,
              lines: a.gl.lines,
            }}
            canApprove={false}
            approveLabel={<T id="waybill.pip.glBeforePay" locale={localeSafe} /> as unknown as string}
            actorCanSeeLines={actorCanSeeGlLines}
            locale={localeSafe}
            disabledReason={<T id="waybill.pip.awaitingFinal" locale={localeSafe} /> as unknown as string}
          />,
        );
      }
    }
  }

  if (pip.key === 'awaiting_disbursement') {
    if (isProcurementArtifacts) {
      const a = artifacts as ProcurementArtifacts;
      if (a.paySlip) {
        cards.push(
          <PipArtifactPanel
            key="payslip"
            kind="paySlip"
            waybillId={waybillId}
            artifact={{
              id: a.paySlip.method ?? 'transfer',
              status: a.paySlip.paid_at ? 'settled' : 'issued',
              href: null,
              finalizedAt: a.paySlip.paid_at,
              finalizedByName: a.paySlip.paid_by_name ?? null,
            }}
            canApprove={actions.canPostSettlement && !!a.glSettlement && !a.glSettlement.finalized_at}
            approveAction={postProcurementSettlementAction}
            approveHiddenInputs={
              a.glSettlement
                ? { journalId: String(a.glSettlement.id), stage: 'disbursed' }
                : { journalId: '0', stage: 'disbursed' }
            }
            approveLabel={<T id="waybill.pip.postSettlement" locale={localeSafe} /> as unknown as string}
            actorCanSeeLines={actorCanSeeGlLines}
            locale={localeSafe}
          />,
        );
      }
    } else {
      const a = artifacts as ExpenseArtifacts;
      if (a.paySlip) {
        cards.push(
          <PipArtifactPanel
            key="payslip"
            kind="paySlip"
            waybillId={waybillId}
            artifact={{
              id: a.paySlip.method ?? 'transfer',
              status: a.paySlip.paid_at ? 'settled' : 'issued',
              href: null,
              finalizedAt: a.paySlip.paid_at,
              finalizedByName: a.paySlip.paid_by_name ?? null,
            }}
            canApprove={actions.canSettle && originId != null}
            approveAction={undefined}
            approveLabel={<T id="waybill.pip.paymentSlip" locale={localeSafe} /> as unknown as string}
            actorCanSeeLines={actorCanSeeGlLines}
            locale={localeSafe}
          />,
        );
      }
    }
  }

  if (pip.key === 'disbursed' || pip.key === 'cfo_authorization') {
    if (isProcurementArtifacts) {
      const a = artifacts as ProcurementArtifacts;
      if (a.glSettlement) {
        cards.push(
          <PipArtifactPanel
            key="gl-settlement"
            kind="gl-settlement"
            waybillId={waybillId}
            artifact={{
              id: `JE #${a.glSettlement.id}`,
              status: a.glSettlement.finalized_at ? 'posted' : 'draft',
              href: null,
              finalizedAt: a.glSettlement.finalized_at,
              finalizedByName: a.glSettlement.finalized_by_name ?? null,
              lines: a.glSettlement.lines,
            }}
            canApprove={actions.canPostSettlement && !a.glSettlement.finalized_at}
            approveAction={postProcurementSettlementAction}
            approveHiddenInputs={{ journalId: String(a.glSettlement.id), stage: 'disbursed' }}
            approveLabel={<T id="waybill.pip.postSettlement" locale={localeSafe} /> as unknown as string}
            actorCanSeeLines={actorCanSeeGlLines}
            locale={localeSafe}
          />,
        );
      }
      if (a.glSettlement && actions.canConfirmSettlement && a.glSettlement.finalized_at) {
        cards.push(
          <PipArtifactPanel
            key="gl-confirm"
            kind="gl-settlement"
            waybillId={waybillId}
            artifact={{
              id: `JE #${a.glSettlement.id}`,
              status: 'finalized',
              href: null,
              finalizedAt: a.glSettlement.finalized_at,
              finalizedByName: a.glSettlement.finalized_by_name ?? null,
              lines: a.glSettlement.lines,
            }}
            canApprove={true}
            approveAction={confirmProcurementGlAction}
            approveHiddenInputs={{ step: 'settlement' }}
            approveLabel={<T id="waybill.pip.confirmGlSettlement" locale={localeSafe} /> as unknown as string}
            actorCanSeeLines={actorCanSeeGlLines}
            locale={localeSafe}
          />,
        );
      }
    } else {
      const a = artifacts as ExpenseArtifacts;
      if (a.gl && a.gl.finalized_at) {
        cards.push(
          <PipArtifactPanel
            key="gl-settlement"
            kind="gl-settlement"
            waybillId={waybillId}
            artifact={{
              id: `JE #${a.gl.id}`,
              status: 'finalized',
              href: null,
              finalizedAt: a.gl.finalized_at,
              finalizedByName: a.gl.finalized_by_name ?? null,
              lines: a.gl.lines,
            }}
            canApprove={actions.canConfirmGl}
            approveAction={confirmGlRecordedAction}
            approveHiddenInputs={{ expenseId: String(originId ?? 0) }}
            approveLabel={<T id="waybill.pip.confirmGl" locale={localeSafe} /> as unknown as string}
            actorCanSeeLines={actorCanSeeGlLines}
            locale={localeSafe}
          />,
        );
      }
    }
  }

  if (cards.length === 0) return null;

  return (
    <ZoneSection
      icon={<span aria-hidden>🧾</span>}
      label={<T id="waybill.pip.artifactsAtStep" locale={localeSafe} />}
      meta={<T id="waybill.pip.artifactsMeta" locale={localeSafe} />}
      tone="cyan"
    >
      <div className="grid grid-cols-1 gap-3 lg:grid-cols-2">{cards}</div>
    </ZoneSection>
  );
}
