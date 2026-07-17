import { EXPENSE_STAGES, PROCUREMENT_STAGES, SALES_STAGES, type WaybillStagePip } from './labels';
import { STAGE_TO_ROLE, normalizeStage, stageRoles, stagePrimaryRole } from '../perm/stages';
export type { WaybillStagePip } from './labels';
export { STAGE_TO_ROLE, stageRoles, stagePrimaryRole };

export interface WaybillEventLite {
  stage_from: string | null;
  stage_to: string | null;
  actor_id: number | null;
  actor_role: string | null;
  kind: string;
  occurred_at: Date;
  sequence: number;
}

export type WaybillDomain = 'expense' | 'procurement' | 'sales';
export type WaybillBucket = 'submission' | 'verification' | 'authorization' | 'disbursement' | 'closed';

export function domainForOrigin(origin: 'expense' | 'pr' | 'po' | 'so'): WaybillDomain {
  if (origin === 'expense') return 'expense';
  if (origin === 'so') return 'sales';
  return 'procurement';
}

export function pipsForDomain(domain: WaybillDomain): WaybillStagePip[] {
  if (domain === 'sales') return SALES_STAGES.pips;
  return domain === 'procurement' ? PROCUREMENT_STAGES.pips : EXPENSE_STAGES.pips;
}

export function findPip(domain: WaybillDomain, key: string): WaybillStagePip | null {
  return pipsForDomain(domain).find((p) => p.key === key) ?? null;
}

export function pipIndex(domain: WaybillDomain, key: string): number {
  const pips = pipsForDomain(domain);
  return pips.findIndex((p) => p.key === key);
}

export function bucketLabel(bucket: WaybillBucket, _lang: 'en' | 'th' | 'de' = 'en'): string {
  return `waybill.bucket.${bucket}`;
}

const STAGE_ROLE_LABEL: Record<string, string> = {
  draft: 'waybill.role.submitter',
  submission: 'waybill.role.submitter',
  dept_verification: 'waybill.role.higherLevel',
  dept_authorization: 'waybill.role.higherLevel',
  accounting_verification: 'waybill.role.accountFinanceReviewer',
  accounting_supervision: 'waybill.role.accountSupervisor',
  accounting_authorization: 'waybill.role.highestAccountFinanceApprover',
  final_authorization: 'waybill.role.financeFinalApproval',
  awaiting_disbursement: 'waybill.role.finance',
  disbursed: 'waybill.role.finance',
  gl_confirmed: 'waybill.role.accountFinanceConfirmer',
  rejected: 'waybill.role.none',
  so_draft: 'waybill.role.salesRep',
  so_sales_review: 'waybill.role.salesSupervisor',
  so_credit_check: 'waybill.role.accountSalesSupervisor',
  so_invoiced: 'waybill.role.accountOfficer',
  so_paid: 'waybill.role.finance',
};

export function stageRoleLabel(stage: string, _lang: 'en' | 'th' | 'de' = 'en'): string {
  return STAGE_ROLE_LABEL[stage] ?? STAGE_ROLE_LABEL[normalizeStage(stage) ?? ''] ?? 'waybill.role.unknown';
}

export function nextStageOf(
  domain: WaybillDomain,
  current: string,
  totalAmountTHB?: number,
): string | null {
  const pips = pipsForDomain(domain);
  const idx = pips.findIndex((p) => p.key === current);
  if (idx < 0 || idx >= pips.length - 1) return null;
  return pips[idx + 1].key;
}

export function nextRoleOf(
  domain: WaybillDomain,
  current: string,
  totalAmountTHB?: number,
): string | null {
  const next = nextStageOf(domain, current, totalAmountTHB);
  if (!next) return null;
  return stagePrimaryRole(next);
}

export function inferActionStage(_domain: WaybillDomain, currentStage: string): string {
  return currentStage;
}

export type PipState = 'passed' | 'active' | 'pending' | 'rejected' | 'skipped';

export function isTerminalStage(stage: string): boolean {
  const canon = normalizeStage(stage) ?? stage;
  return canon === 'disbursed'
    || canon === 'rejected'
    || canon === 'so_paid'
    || canon === 'gl_confirmed'
    || canon === 'so_invoiced';
}

export function computePipState(
  pip: WaybillStagePip,
  idx: number,
  curIdx: number,
  currentStage: string,
  status?: string,
): PipState {
  if (currentStage === 'rejected' || status === 'rejected') return 'rejected';
  if (status === 'completed') {
    return pip.key === 'rejected' ? 'pending' : 'passed';
  }
  if (curIdx < 0) return pip.key === 'rejected' ? 'rejected' : 'pending';
  if (idx < curIdx) return 'passed';
  if (idx === curIdx) return 'active';
  return 'pending';
}

export function eventsForPip(
  events: ReadonlyArray<WaybillEventLite>,
  pipKey: string,
  limit = 2,
): WaybillEventLite[] {
  const touching = events.filter((e) => e.stage_from === pipKey || e.stage_to === pipKey);
  touching.sort((a, b) => (b.sequence ?? 0) - (a.sequence ?? 0));
  return touching.slice(0, limit);
}

export function lastAdvancedEvent<T extends WaybillEventLite>(
  events: ReadonlyArray<T>,
  pipKey: string,
): T | null {
  const advanced = events.find((e) => e.kind === 'advanced' && e.stage_to === pipKey);
  if (advanced) return advanced;
  const toStage = events.find((e) => e.stage_to === pipKey);
  if (toStage) return toStage;
  return null;
}

export function pipsForKindLayout(domain: 'expense' | 'procurement' | 'sales'): WaybillStagePip[] {
  return pipsForDomain(domain);
}