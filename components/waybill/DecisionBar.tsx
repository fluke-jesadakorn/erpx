'use client';

import React from 'react';
import { stageRoleLabel } from '@/waybill/derive';
import { roleDisplay } from './ui';
import { AiRecommendChip } from './AiRecommendChip';
import { useSecondaryLocale } from '@/components/i18n/SecondaryLocaleProvider';
import { T } from '@/components/i18n/T';

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
}

const STAGE_GLYPHS: Record<string, string> = {
  submission: '📤',
  dept_verification: '🔎',
  dept_authorization: '🪪',
  accounting_verification: '🧮',
  accounting_supervision: '🧮',
  accounting_authorization: '🧮',
  final_authorization: '🔒',
  disbursement_authorization: '💳',
  cfo_authorization: '👔',
  ceo_authorization: '👑',
  awaiting_disbursement: '💸',
  disbursed: '✅',
  rejected: '✗',
  draft: '📝',
  so_draft: '📝',
  so_sales_review: '🛡️',
  so_credit_check: '🔍',
  so_invoiced: '🧾',
  so_paid: '💰',
};

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
      className="sticky top-[calc(var(--navbar-h,56px)+12px)] z-30 -mx-1 rounded-2xl border border-cyan-500/40 bg-slate-950/95 p-3 shadow-2xl shadow-cyan-500/20 backdrop-blur"
    >
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="min-w-0 flex-1 space-y-1">
          <div className="flex items-center gap-2 text-xs font-mono uppercase tracking-widest text-cyan-300">
            <span aria-hidden>{STAGE_GLYPHS[currentStage] ?? '⚙️'}</span>
            <span><T id="waybill.decisions.currentStage" /></span>
            <span className="text-slate-600">·</span>
            <span className="text-white">
              <T id={`waybill.stage.${toCamel(currentStage)}`} />
            </span>
            <span className="text-slate-600">·</span>
            <span className="text-slate-400">
              <T id="waybill.decisions.requires" />{' '}
              <span className="font-bold text-cyan-200">{requiredRole}</span>
            </span>
          </div>
          {actorRole && (
            <div className="text-xs font-mono text-slate-500">
              <T id="waybill.decisions.youAre" />{' '}
              <span className="text-slate-300">{roleDisplay(actorRole, locale)}</span>
            </div>
          )}
        </div>

        <div className="flex flex-wrap items-center gap-2">
          <a
            href={`/api/waybill/${waybillId}/attachments/file?key=waybill-attachments/${waybillId}/combined&include=mockups`}
            target="_blank"
            rel="noopener"
            data-testid={`bar-step-pdf-${waybillId}`}
            className="inline-flex items-center gap-1.5 rounded-lg border border-cyan-500/40 bg-cyan-500/10 px-3 py-1.5 text-sm font-mono text-cyan-200 hover:bg-cyan-500/30"
            title="Download step files (PR, PO, GL, payment slip) as one PDF"
          >
            ⤓ <T id="waybill.stepPdf" />
          </a>
          <AiRecommendChip
            waybillId={waybillId}
            amount={amount}
            currentStage={currentStage}
            vendorName={vendorName}
          />
          <span className="hidden text-xs font-mono uppercase tracking-widest text-slate-500 sm:inline">
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
