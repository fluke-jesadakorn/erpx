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

export function bucketLabel(bucket: WaybillBucket, lang: 'en' | 'th' | 'de' = 'en'): string {
  const map: Record<WaybillBucket, { en: string; th: string; de: string }> = {
    submission:    { en: 'Submission',     th: 'ยื่นเอกสาร',      de: 'Einreichung' },
    verification:  { en: 'Verification',   th: 'ตรวจสอบ',         de: 'Prüfung' },
    authorization: { en: 'Authorization',  th: 'ลงนามอนุมัติ',      de: 'Genehmigung' },
    disbursement:  { en: 'Disbursement',   th: 'จ่ายเงิน',         de: 'Auszahlung' },
    closed:        { en: 'Closed',         th: 'ปิดรายการ',         de: 'Abgeschlossen' },
  };
  return map[bucket][lang] ?? map[bucket].en;
}

const STAGE_ROLE_LABEL_EN: Record<string, string> = {
  draft: 'Submitter', submission: 'Submitter',
  dept_verification: 'Higher level', dept_authorization: 'Higher level',
  accounting_verification: 'Account / Finance reviewer',
  accounting_supervision: 'Account supervisor (QC)',
  accounting_authorization: 'Highest account / finance approver',
  final_authorization: 'Finance final approval',
  awaiting_disbursement: 'Finance', disbursed: 'Finance',
  gl_confirmed: 'Account / Finance confirmer',
  rejected: '—',
  so_draft: 'Sales rep', so_sales_review: 'Sales supervisor',
  so_credit_check: 'Account / Sales supervisor', so_invoiced: 'Account officer', so_paid: 'Finance',
};
const STAGE_ROLE_LABEL_TH: Record<string, string> = {
  draft: 'ผู้ส่ง', submission: 'ผู้ส่ง',
  dept_verification: 'ระดับสูง', dept_authorization: 'ระดับสูง',
  accounting_verification: 'ผู้ตรวจบัญชี/การเงิน',
  accounting_supervision: 'หน.บัญชี (QC)',
  accounting_authorization: 'ผู้อนุมัติสูงสุดบัญชี/การเงิน',
  final_authorization: 'การเงินขั้นสุดท้าย',
  awaiting_disbursement: 'การเงิน', disbursed: 'การเงิน',
  gl_confirmed: 'ผู้ยืนยันบัญชี/การเงิน',
  rejected: '—',
  so_draft: 'เซลล์', so_sales_review: 'หัวหน้าทีมขาย',
  so_credit_check: 'บัญชี/หน.ทีมขาย', so_invoiced: 'เจ้าหน้าที่บัญชี', so_paid: 'การเงิน',
};
const STAGE_ROLE_LABEL_DE: Record<string, string> = {
  draft: 'Einreicher', submission: 'Einreicher',
  dept_verification: 'Höhere Ebene', dept_authorization: 'Höhere Ebene',
  accounting_verification: 'Buchhaltung/Finanz-Prüfer',
  accounting_supervision: 'Buchhaltungsleiter (QC)',
  accounting_authorization: 'Höchster Buchhaltungs-/Finanz-Genehmiger',
  final_authorization: 'Finanz-Endgenehmigung',
  awaiting_disbursement: 'Finanzen', disbursed: 'Finanzen',
  gl_confirmed: 'Buchhaltung/Finanz-Bestätiger',
  rejected: '—',
  so_draft: 'Verkaufsmitarbeiter', so_sales_review: 'Verkaufsleiter',
  so_credit_check: 'Buchhaltung/Verkaufsleiter', so_invoiced: 'Buchhalter', so_paid: 'Finanzen',
};

export function stageRoleLabel(stage: string, lang: 'en' | 'th' | 'de' = 'en'): string {
  const map = lang === 'th' ? STAGE_ROLE_LABEL_TH : lang === 'de' ? STAGE_ROLE_LABEL_DE : STAGE_ROLE_LABEL_EN;
  return map[stage] ?? map[normalizeStage(stage) ?? ''] ?? '—';
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

export function pipsForKindLayout(domain: 'expense' | 'procurement' | 'sales'): WaybillStagePip[] {
  return pipsForDomain(domain);
}