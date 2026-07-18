'use client';

import React from 'react';
import { stageRoleLabel } from '@/waybill/derive';
import { roleDisplay } from './ui';
import { AiRecommendChip } from './AiRecommendChip';
import { useSecondaryLocale } from '@/components/i18n/SecondaryLocaleProvider';
import { T } from '@/components/i18n/T';
import { CircleDot, Download } from 'lucide-react';
import { claimExpenseStageAction } from '@/app/actions/waybill';

interface Props {
  waybillId: string;
  currentStage: string;
  status: string;
  amount?: number | string | null;
  vendorName?: string | null;
  canAct: boolean;
  canFinalApprove: boolean;
  canSettle: boolean;
  canConfirmGl: boolean;
  isFinalApproval: boolean;
  isAwaitingDisbursement: boolean;
  isDisbursed: boolean;
  isRejected: boolean;
  canApproveSalesAtReview?: boolean;
  canApproveSalesAtCredit?: boolean;
  canIssueSalesInvoice?: boolean;
  canPostSalesGlVat?: boolean;
  canPostSalesGlAccrual?: boolean;
  canPostSalesGlSettlement?: boolean;
  canConfirmSalesGl?: boolean;
  actorRole: string | null;
  claim?: {
    claimedBy: number | null;
    claimedByName: string | null;
    isMine: boolean;
    canClaim: boolean;
  } | null;
}

export function DecisionBar({
  waybillId,
  currentStage,
  status: _status,
  amount,
  vendorName,
  canAct,
  canFinalApprove,
  canSettle,
  canConfirmGl,
  isFinalApproval: _isFinalApproval,
  isAwaitingDisbursement: _isAwaitingDisbursement,
  isDisbursed: _isDisbursed,
  isRejected,
  canApproveSalesAtReview,
  canApproveSalesAtCredit,
  canIssueSalesInvoice,
  canPostSalesGlVat,
  canPostSalesGlAccrual,
  canPostSalesGlSettlement,
  canConfirmSalesGl,
  actorRole,
  claim,
}: Props) {
  const locale = useSecondaryLocale();
  void _status;
  void _isFinalApproval;
  void _isAwaitingDisbursement;
  void _isDisbursed;
  void isRejected;
  if (isRejected) return null;

  const salesPower =
    !!canApproveSalesAtReview ||
    !!canApproveSalesAtCredit ||
    !!canIssueSalesInvoice ||
    !!canSettle ||
    !!canPostSalesGlVat ||
    !!canPostSalesGlAccrual ||
    !!canPostSalesGlSettlement ||
    !!canConfirmSalesGl;
  const noPower =
    !canAct && !canFinalApprove && !canSettle && !canConfirmGl && !salesPower;
  if (noPower) return null;

  const requiredRole = stageRoleLabel(currentStage, locale);

  return (
    <aside
      role="region"
      aria-label="Decision bar"
      className="panel-floating sticky top-[4.75rem] z-sticky -mx-1 border-info/40 p-3"
    >
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="min-w-0 flex-1 space-y-1">
          <div className="flex items-center gap-2 text-xs font-mono uppercase tracking-widest text-info">
            <CircleDot size={13} aria-hidden />
            <span><T id="waybill.decisions.currentStage" /></span>
            <span className="text-mute">·</span>
            <span className="text-ink">
              <T id={`waybill.stage.${toCamel(currentStage)}`} />
            </span>
            <span className="text-mute">·</span>
            <span className="text-ink-2">
              <T id="waybill.decisions.requires" />{' '}
              <span className="font-bold text-info-soft">{requiredRole}</span>
            </span>
          </div>
          {actorRole && (
            <div className="text-xs font-mono text-mute">
              <T id="waybill.decisions.youAre" />{' '}
              <span className="text-ink-2">{roleDisplay(actorRole, locale)}</span>
            </div>
          )}
        </div>

        <div className="flex flex-wrap items-center gap-2">
          {claim?.canClaim ? (
            <form action={claimExpenseStageAction}>
              <input type="hidden" name="waybillId" value={waybillId} />
              <button
                type="submit"
                className="inline-flex items-center gap-1.5 rounded-lg border border-positive/55 bg-positive px-3 py-1.5 text-sm font-bold text-paper hover:bg-positive-strong"
              >
                Claim task
              </button>
            </form>
          ) : claim?.isMine ? (
            <span className="rounded-lg border border-positive/45 bg-positive-soft px-3 py-1.5 text-sm font-bold text-positive">
              Claimed by you
            </span>
          ) : claim?.claimedBy ? (
            <span className="rounded-lg border border-caution/45 bg-caution-soft px-3 py-1.5 text-sm text-caution">
              Claimed by {claim.claimedByName ?? `user #${claim.claimedBy}`}
            </span>
          ) : null}
          <a
            href={`/api/waybill/${waybillId}/attachments/file?key=waybill-attachments/${waybillId}/combined&include=mockups`}
            target="_blank"
            rel="noopener"
            data-testid={`bar-step-pdf-${waybillId}`}
            className="inline-flex items-center gap-1.5 rounded-lg border border-info/55 bg-info text-paper px-3 py-1.5 text-sm font-mono hover:bg-info-strong"
            title="Download step files (PR, PO, GL, payment slip) as one PDF"
          >
            <Download size={14} aria-hidden /> <T id="waybill.stepPdf" />
          </a>
          <AiRecommendChip
            waybillId={waybillId}
            amount={amount}
            currentStage={currentStage}
            vendorName={vendorName}
          />
          <span className="hidden text-xs font-mono uppercase tracking-widest text-mute sm:inline">
            <T id="waybill.decisions.approveRejectHint" />
          </span>
        </div>
      </div>
    </aside>
  );
}

function toCamel(snake: string): string {
  const parts = snake.split('_');
  return parts[0] + parts.slice(1).map((p) => p.charAt(0).toUpperCase() + p.slice(1)).join('');
}
