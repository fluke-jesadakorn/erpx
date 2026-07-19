import { roleNameOf } from '../perm/grammar';

export type FinanceOrigin = 'expense' | 'pr' | 'po';

export function isExecutiveRole(roleName: string | null | undefined): boolean {
  if (!roleName) return false;
  return ['cfo', 'ceo'].includes(roleNameOf(roleName).toLowerCase());
}

export function expenseEntryStage(roleName: string | null | undefined): 'department_approval' | 'accounting_review' {
  return isExecutiveRole(roleName) ? 'accounting_review' : 'department_approval';
}

export function procurementResubmitStage(roleName: string | null | undefined): 'submission' | 'accounting_authorization' {
  return isExecutiveRole(roleName) ? 'accounting_authorization' : 'submission';
}

export function nextProcurementStage(
  currentStage: string,
  roleName: string | null | undefined,
  isSubmitter: boolean,
): string | null {
  if (currentStage === 'submission' && isSubmitter && isExecutiveRole(roleName)) {
    return 'accounting_authorization';
  }
  const order = ['submission', 'dept_authorization', 'accounting_authorization', 'cfo_authorization', 'disbursed'];
  const idx = order.indexOf(currentStage);
  if (idx < 0 || idx >= order.length - 1) return null;
  return order[idx + 1];
}

export function skippedDepartmentStage(origin: FinanceOrigin | 'so'): 'department_approval' | 'dept_authorization' {
  return origin === 'expense' ? 'department_approval' : 'dept_authorization';
}
