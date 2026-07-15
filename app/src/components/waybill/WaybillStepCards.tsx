import React from 'react';
void React;
import { StepCard } from '@/components/StepCard';
import type { WaybillEventRow } from '@folio-lib/waybill/events';
import type { WaybillAttachmentRow } from '@folio-lib/waybill/attachments';
import type {
  ActedUserEntry,
  ApproverRow,
  ExpenseArtifacts,
  ExpenseFullPicture,
  ProcurementArtifacts,
  SalesArtifacts,
  WaybillRow,
} from '@folio-lib/waybill/queries';
import type { PipState, WaybillDomain } from '@folio-lib/waybill/derive';
import { pipsForDomain, pipIndex, domainForOrigin } from '@folio-lib/waybill/derive';
import type { VisionModel } from '@folio-lib/ai/loadVisionModels';
import { PipCard } from './pip/PipCard';
import { Bilingual } from '@/components/i18n/Bilingual';
import type { BilingualText } from '@folio-lib/i18n/types';

export type StepperArtifacts =
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

export interface WaybillStepCardsProps {
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
  artifacts: StepperArtifacts;
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

function stateToTone(
  state: PipState,
  hasPerm: boolean,
): 'slate' | 'cyan' | 'emerald' | 'amber' {
  if (state === 'passed') return 'emerald';
  if (state === 'rejected') return 'amber';
  if (state === 'active') return hasPerm ? 'cyan' : 'slate';
  return 'slate';
}

function stateAccent(state: PipState, hasPerm: boolean): 'your-turn' | null {
  if (state === 'active' && hasPerm) return 'your-turn';
  return null;
}

function stateBadge(state: PipState, hasPerm: boolean): BilingualText {
  switch (state) {
    case 'passed':
      return { en: '✓ Done', th: '✓ เสร็จ', de: '✓ Erledigt' };
    case 'active':
      return hasPerm
        ? { en: '◉ Your turn', th: '◉ ถึงคิวคุณ', de: '◉ Sie sind dran' }
        : { en: '◌ Locked', th: '◌ รอผู้อนุมัติ', de: '◌ Wartet auf Genehmiger' };
    case 'rejected':
      return { en: '✗ Stop', th: '✗ หยุด', de: '✗ Stopp' };
    case 'skipped':
      return { en: '— Skip', th: '— ข้าม', de: '— Übersprungen' };
    default:
      return { en: '○ Pending', th: '○ รอ', de: '○ Ausstehend' };
  }
}

export function WaybillStepCards({
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
  artifacts,
  actorCanSeeGlLines,
  originId,
  visionModels,
  actions,
  flags,
  rejection,
  ui,
}: WaybillStepCardsProps) {
  const domain: WaybillDomain = domainForOrigin(wb.origin);
  const pips = pipsForDomain(domain);
  const curIdx = pipIndex(domain, currentStage);
  const isPageRejected = status === 'rejected';
  const locale = (ui.locale ?? 'th') as 'th' | 'de';

  return (
    <ol className="space-y-6" aria-label="Waybill steps">
      {pips.map((pip, idx) => {
        const state = pipStateFor(pip.key, idx, curIdx, currentStage, status);
        const approvers = approversByStage[pip.key] ?? [];
        const actedUsers = actedUsersByStage[pip.key] ?? [];
        const focusedIsThisRejected = state === 'rejected';

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
            !!actions.canConfirmSalesGl);
        const dimmed = state !== 'passed' && !(state === 'active' && hasAnyPerm);

        const pipEvents = events.filter(
          (e) => e.stage_from === pip.key || e.stage_to === pip.key,
        );
        const badgeText = stateBadge(state, hasAnyPerm);

        return (
          <li key={pip.key} id={`pip-${pip.key}`}>
            <StepCard
              n={idx + 1}
              icon={pip.emoji}
              title={pip.en}
              titleTh={locale === 'de' ? pip.de : pip.th}
              hint={
                <Bilingual en={pip.description_en} th={pip.description_th} locale={locale} />
              }
              done={state === 'passed'}
              active={state === 'active'}
              tone={stateToTone(state, hasAnyPerm)}
              accent={stateAccent(state, hasAnyPerm)}
              dimmed={dimmed}
              badge={
                !(state === 'active' && hasAnyPerm) ? (
                  <span
                    className={[
                      'rounded-full border px-2 py-0.5 text-xs font-mono font-bold uppercase tracking-wider',
                      state === 'passed'
                        ? 'border-emerald-500/40 bg-emerald-500/15 text-emerald-200'
                        : state === 'active'
                          ? 'border-slate-700 bg-slate-900/60 text-slate-400'
                          : state === 'rejected'
                            ? 'border-amber-500/50 bg-amber-500/15 text-amber-200'
                            : 'border-slate-700 bg-slate-900/60 text-slate-400',
                    ].join(' ')}
                  >
                    <Bilingual en={badgeText.en} th={badgeText.th} de={badgeText.de} locale={locale} />
                  </span>
                ) : null
              }
            >
              <div className="space-y-10">
                <PipCard
                  waybillId={waybillId}
                  pip={pip}
                  state={state}
                  currentStage={currentStage}
                  events={pipEvents}
                  attachments={_attachments}
                  approvers={approvers}
                  actedUsers={actedUsers}
                  artifacts={artifacts as any}
                  actorCanSeeGlLines={actorCanSeeGlLines}
                  visionModels={visionModels}
                  isRejected={focusedIsThisRejected || isPageRejected}
                  domain={domain}
                  rejectionReason={
                    pip.key === 'rejected' || focusedIsThisRejected ? rejection.reason : null
                  }
                  rejectionActor={
                    pip.key === 'rejected' || focusedIsThisRejected ? rejection.actor : null
                  }
                  isSubmitter={flags.isSubmitter}
                  action={ui.action}
                  actionStage={ui.actionStage}
                  originId={originId}
                  locale={locale}
                  actions={actions}
                />
              </div>
            </StepCard>
          </li>
        );
      })}
    </ol>
  );
}
