import { expenseEntryStage } from './routing';

export const EXECUTIVE_THRESHOLD_THB = 200_000;

export type ExpenseStage =
  | 'submission'
  | 'department_approval'
  | 'accounting_review'
  | 'accounting_approval'
  | 'executive_approval'
  | 'payment'
  | 'settlement'
  | 'completed'
  | 'rejected';

export interface ExpenseRuleActor {
  id: number;
  departmentId: string | null;
  rank: number;
  roleName: string;
}

export interface ExpenseRuleContext {
  stage: ExpenseStage;
  amount: number;
  submitterId: number;
  submitterDepartmentId: string | null;
  departmentHeadId: number | null;
  departmentTopRank: number | null;
  accountingHeadId: number | null;
  accountingTopRank: number | null;
  accrualPreparerId: number | null;
}

export function requiresExecutiveApproval(amount: number): boolean {
  return amount > EXECUTIVE_THRESHOLD_THB;
}

export function nextExpenseStage(
  stage: ExpenseStage,
  amount: number,
  submitterRoleName?: string | null,
): ExpenseStage | null {
  if (stage === 'submission') return expenseEntryStage(submitterRoleName);
  if (stage === 'department_approval') return 'accounting_review';
  if (stage === 'accounting_review') return 'accounting_approval';
  if (stage === 'accounting_approval') return requiresExecutiveApproval(amount) ? 'executive_approval' : 'payment';
  if (stage === 'executive_approval') return 'payment';
  if (stage === 'payment') return 'settlement';
  if (stage === 'settlement') return 'completed';
  return null;
}

export function evaluateExpenseStageRule(
  actor: ExpenseRuleActor,
  ctx: ExpenseRuleContext,
  stage: ExpenseStage = ctx.stage,
): { allow: true; reason: string } | { allow: false; reason: string } {
  if (stage === 'department_approval') {
    if (!ctx.submitterDepartmentId || actor.departmentId !== ctx.submitterDepartmentId) {
      return { allow: false, reason: 'Department approval requires the submitter department' };
    }
    const designatedHead = ctx.departmentHeadId === actor.id;
    const highest = ctx.departmentTopRank !== null && actor.rank === ctx.departmentTopRank;
    if (!designatedHead && !highest) {
      return { allow: false, reason: 'Only the department head or highest-ranked active member may approve' };
    }
    if (actor.id === ctx.submitterId && !designatedHead) {
      return { allow: false, reason: 'Only a designated department head may self-approve' };
    }
  }
  if (stage === 'accounting_review' && actor.departmentId !== 'accounting') {
    return { allow: false, reason: 'Accounting review requires an Accounting member' };
  }
  if (stage === 'accounting_approval') {
    if (actor.departmentId !== 'accounting') {
      return { allow: false, reason: 'Accounting approval requires an Accounting member' };
    }
    const head = ctx.accountingHeadId === actor.id;
    const highest = ctx.accountingTopRank !== null && actor.rank === ctx.accountingTopRank;
    if (!head && !highest) {
      return { allow: false, reason: 'Accounting approval requires the Accounting head or highest-ranked active member' };
    }
    if (actor.id === ctx.submitterId) {
      return { allow: false, reason: 'Accounting approver must differ from submitter' };
    }
    if (actor.id === ctx.accrualPreparerId && actor.roleName !== 'accounting_manager') {
      return { allow: false, reason: 'Only the Accounting Manager may approve their own accrual draft' };
    }
  }
  if (stage === 'executive_approval') {
    if (!requiresExecutiveApproval(ctx.amount)) {
      return { allow: false, reason: 'Executive approval is skipped at or below THB 200,000' };
    }
    if (!['cfo', 'ceo'].includes(actor.roleName)) {
      return { allow: false, reason: 'Executive approval requires either CFO or CEO' };
    }
    if (actor.id === ctx.submitterId) {
      return { allow: false, reason: 'An executive cannot approve their own claim' };
    }
  }
  if (stage === 'payment') {
    if (actor.departmentId !== 'finance') {
      return { allow: false, reason: 'Payment requires a Finance member' };
    }
    if (actor.id === ctx.submitterId) {
      return { allow: false, reason: 'The submitter cannot pay their own claim' };
    }
  }
  if (stage === 'settlement' && actor.departmentId !== 'accounting') {
    return { allow: false, reason: 'Settlement requires an Accounting member' };
  }
  return { allow: true, reason: 'Expense stage constraints satisfied' };
}
