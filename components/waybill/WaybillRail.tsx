import React from 'react';
import Link from 'next/link';
import { Check, UserCheck, Zap } from 'lucide-react';
import type { WaybillEventRow } from '@/waybill/events';
import type { WaybillAttachmentRow } from '@/waybill/attachments';
import type {
  ActedUserEntry,
  ApproverRow,
  ExpenseArtifacts,
  ExpenseFullPicture,
  ProcurementArtifacts,
  SalesArtifacts,
  WaybillRow,
} from '@/waybill/queries';
import type { PipState, WaybillDomain } from '@/waybill/derive';
import { pipsForDomain, pipIndex, domainForOrigin } from '@/waybill/derive';
import type { VisionModel } from '@/ai/loadVisionModels';
import { Badge } from '@/components/ui';
import { ApproverStack } from './ApproverStack';
import { T } from '@/components/i18n/T';
import { recordSalesPaymentAction } from '@/app/actions/sales';

export type RailArtifacts =
  | ExpenseArtifacts
  | ProcurementArtifacts
  | SalesArtifacts
  | null;

export interface ActionsBag {
  canAct: boolean;
  canAttach: boolean;
  canSettle: boolean;
  canFinalApprove: boolean;
  canConfirmGl: boolean;
  canReCall: boolean;
  canSaveAccrual: boolean;
  canPostAccrual: boolean;
  canConfirmAccrual: boolean;
  canPostSettlement: boolean;
  canConfirmSettlement: boolean;
  canApproveSalesAtReview?: boolean;
  canApproveSalesAtCredit?: boolean;
  canIssueSalesInvoice?: boolean;
  canPostSalesGlVat?: boolean;
  canPostSalesGlAccrual?: boolean;
  canPostSalesGlSettlement?: boolean;
  canConfirmSalesGl?: boolean;
  canRecordSalesPayment?: boolean;
}

export interface FlagsBag {
  isSubmitter: boolean;
  isFinalApproval: boolean;
  isDisbursed: boolean;
}

export interface RejectionBag {
  reason: string | null;
  actor: { user_id: number; fullname: string; role: string | null } | null;
}

export interface UiBag {
  action: string | null;
  actionStage: string | null;
  locale?: 'th' | 'de';
}

export interface WaybillRailProps {
  wb: WaybillRow;
  waybillId: string;
  currentStage: string;
  status: string;
  events: WaybillEventRow[];
  attachments: WaybillAttachmentRow[];
  approversByStage: Record<string, ApproverRow[]>;
  actedUsersByStage: Record<string, ActedUserEntry[]>;
  expensePicture: ExpenseFullPicture | null;
  hasGlConfirmed: boolean;
  artifacts: RailArtifacts;
  actorCanSeeGlLines: boolean;
  originId: number | null;
  visionModels: VisionModel[];
  actions: ActionsBag;
  flags: FlagsBag;
  rejection: RejectionBag;
  ui: UiBag;
}

function pipStateFor(
  pipKey: string,
  idx: number,
  curIdx: number,
  currentStage: string,
  status: string,
): PipState {
  if (currentStage === 'rejected' || status === 'rejected') {
    return pipKey === 'rejected' ? 'rejected' : 'pending';
  }
  if (status === 'completed') {
    return pipKey === 'rejected' ? 'pending' : 'passed';
  }
  if (curIdx < 0) return 'pending';
  if (idx < curIdx) return 'passed';
  if (idx === curIdx) return 'active';
  return 'pending';
}

function stateBadgeKey(state: PipState, hasPerm: boolean): string {
  switch (state) {
    case 'passed':
      return 'waybill.pip.done';
    case 'active':
      return hasPerm ? 'waybill.pip.yourTurn' : 'waybill.pip.locked';
    case 'rejected':
      return 'waybill.pip.stop';
    case 'skipped':
      return 'waybill.pip.skip';
    default:
      return 'waybill.pip.pending';
  }
}

const DOT_TONE: Record<PipState, string> = {
  passed: 'bg-positive',
  active: 'bg-accent',
  pending: 'bg-mute',
  rejected: 'bg-critical',
  skipped: 'bg-mute',
};

const DOT_RING: Record<PipState, string> = {
  passed: 'border-positive',
  active: 'border-accent',
  pending: 'border-rule-strong',
  rejected: 'border-critical',
  skipped: 'border-rule',
};

const LINE_TONE: Record<PipState, string> = {
  passed: 'bg-positive',
  active: 'bg-rule',
  pending: 'bg-rule',
  rejected: 'bg-rule',
  skipped: 'bg-rule',
};

const LINE_FUTURE = 'bg-rule border-dashed';

export function WaybillRail({
  wb,
  waybillId,
  currentStage,
  status,
  events,
  attachments: _attachments,
  approversByStage,
  actedUsersByStage,
  expensePicture: _expensePicture,
  hasGlConfirmed: _hasGlConfirmed,
  artifacts: _artifacts,
  actorCanSeeGlLines: _actorCanSeeGlLines,
  originId: _originId,
  visionModels: _visionModels,
  actions,
  flags,
  rejection: _rejection,
  ui,
}: WaybillRailProps) {
  const domain: WaybillDomain = domainForOrigin(wb.origin);
  const pips = pipsForDomain(domain);
  const curIdx = pipIndex(domain, currentStage);
  const isPageRejected = status === 'rejected';
  const locale = (ui.locale ?? 'th') as 'th' | 'de';

  const arSlipAttached =
    _artifacts != null &&
    typeof _artifacts === 'object' &&
    'ar_receipt' in _artifacts &&
    (_artifacts as SalesArtifacts).ar_receipt != null;

  const canRecordSalesPaymentDerived =
    !!actions.canAct &&
    currentStage === 'so_paid' &&
    wb.origin === 'so' &&
    arSlipAttached;

  const canRecordSalesPayment =
    actions.canRecordSalesPayment ?? canRecordSalesPaymentDerived;

  return (
    <ol className="space-y-0" aria-label="Waybill rail">
      {pips.map((pip, idx) => {
        const state = pipStateFor(pip.key, idx, curIdx, currentStage, status);
        const approvers = approversByStage[pip.key] ?? [];
        const actedUsers = actedUsersByStage[pip.key] ?? [];

        const isCurrentStage = idx === curIdx;
        const hasAnyPerm: boolean =
          isCurrentStage &&
          (!!actions.canAct ||
            !!actions.canAttach ||
            !!actions.canSettle ||
            !!actions.canFinalApprove ||
            !!actions.canConfirmGl ||
            !!actions.canApproveSalesAtReview ||
            !!actions.canApproveSalesAtCredit ||
            !!actions.canIssueSalesInvoice ||
            !!actions.canPostSalesGlVat ||
            !!actions.canPostSalesGlAccrual ||
            !!actions.canPostSalesGlSettlement ||
            !!actions.canConfirmSalesGl ||
            !!canRecordSalesPayment);

        const pipEvents = events.filter(
          (e) => e.stage_from === pip.key || e.stage_to === pip.key,
        );
        const isActive = state === 'active';
        const isPassed = state === 'passed';
        const isPending = state === 'pending';
        const isFuture = idx > curIdx && !isActive;
        const dimmed = state !== 'passed' && !(state === 'active' && hasAnyPerm);
        const lastActor = actedUsers.length > 0 ? actedUsers[actedUsers.length - 1] : null;

        return (
          <li
            key={pip.key}
            id={`pip-${pip.key}`}
            className="relative pl-10 pb-8 last:pb-0"
          >
            <div className="absolute left-3 top-0 bottom-0 w-px flex flex-col items-center">
              <span
                aria-hidden
                className={[
                  'z-10 mt-1 inline-flex h-6 w-6 items-center justify-center rounded-full border-2',
                  DOT_RING[state],
                  DOT_TONE[state],
                  isActive && hasAnyPerm ? 'shadow-[0_0_0_4px_var(--accent-soft)]' : '',
                  isActive ? 'animate-pulse' : '',
                ].join(' ')}
              >
                {isPassed ? (
                  <Check size={12} className="text-paper-2" strokeWidth={3} />
                ) : isActive && hasAnyPerm ? (
                  <Zap size={12} className="text-paper-2" strokeWidth={3} />
                ) : null}
              </span>
              {idx < pips.length - 1 && (
                <span
                  aria-hidden
                  className={[
                    'flex-1 w-px',
                    isFuture ? LINE_FUTURE : LINE_TONE[state],
                  ].join(' ')}
                />
              )}
            </div>

            <div
              className={[
                'flex flex-col gap-2 rounded-md border border-rule bg-paper-2 p-4',
                dimmed ? 'opacity-60' : '',
              ].join(' ')}
            >
              <div className="flex flex-wrap items-start justify-between gap-2">
                <div className="min-w-0 flex-1">
                  <div className="flex flex-wrap items-center gap-2">
                    <span className="text-xs font-mono uppercase tracking-widest text-mute">
                      #{idx + 1}
                    </span>
                    <span className="text-sm font-semibold text-ink">
                      <T id={pip.label} />
                    </span>
                    <Badge
                      tone={
                        state === 'passed' ? 'positive' :
                        state === 'active' ? 'accent' :
                        state === 'rejected' ? 'critical' :
                        'neutral'
                      }
                      size="sm"
                    >
                      <T id={stateBadgeKey(state, hasAnyPerm)} />
                    </Badge>
                    {flags.isSubmitter && state === 'rejected' && (
                      <span className="text-xs font-mono text-caution">· resubmit available</span>
                    )}
                  </div>
                  <p className="mt-0.5 text-xs text-ink-2">
                    <T id={pip.description} />
                  </p>
                </div>
                {approvers.length > 0 && (
                  <ApproverStack approvers={approvers} />
                )}
              </div>

              {(isPassed || (isActive && hasAnyPerm)) && lastActor && (
                <div className="flex flex-wrap items-center gap-2 text-xs font-mono text-mute">
                  <UserCheck size={12} aria-hidden />
                  <span>
                    {lastActor.fullname || `#${lastActor.user_id}`}
                    {' · '}
                    <T id="waybill.pip.done" />
                    {pipEvents.length > 0 && (
                      <>
                        {' · '}
                        <Link
                          href={`/waybill/${waybillId}/audit`}
                          className="text-accent hover:underline"
                        >
                          {pipEvents.length} events
                        </Link>
                      </>
                    )}
                  </span>
                </div>
              )}

              {!hasAnyPerm && isPending && (
                <div className="text-xs font-mono text-mute">
                  <T id="waybill.pip.locked" />
                </div>
              )}
              {isPageRejected && (
                <div className="text-xs font-mono text-critical">
                  <T id="waybill.pip.stop" />
                </div>
              )}

              {pip.key === 'so_paid' && canRecordSalesPayment && isCurrentStage && (
                <form
                  action={recordSalesPaymentAction}
                  className="mt-2 rounded-md border border-positive bg-positive-soft p-3"
                >
                  <input type="hidden" name="waybillId" value={waybillId} />
                  <button
                    type="submit"
                    data-testid={`panel-record-sales-payment-${waybillId}`}
                    className="inline-flex w-full items-center justify-center gap-2 rounded-lg bg-positive px-4 py-2.5 text-sm font-mono font-bold text-ink shadow-md shadow-positive transition hover:bg-positive-strong"
                  >
                    <span aria-hidden>💰</span>
                    <span>Record to GL · บันทึก GL</span>
                  </button>
                </form>
              )}
              {void locale}
            </div>
          </li>
        );
      })}
    </ol>
  );
}

export default WaybillRail;