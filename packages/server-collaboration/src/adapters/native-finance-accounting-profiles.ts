import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import { nativeSystemNotificationStatement } from './native-system-notification';

/** Native-owned finance accounting-profile statements. */
export function financeAccountingProfiles(db: SqlDatabase, companyId: string) {
  const call = (kind: string, key: string, values: unknown[]): Promise<SqlStatement> =>
    nativeSystemNotificationStatement(db, companyId,
      { userId: '', recordType: '', recordId: '', recordVersion: 0, message: '', kind, values_json: JSON.stringify(values) }, key, 0);
  return {
    create: (f: {
      id: string; companyId: string; statutoryChartId: string; standard: string; fiscalStartMonth: number;
      effectiveDate: string; eligibilityEvidence: string; policyEvidence: string; taxBasis: string;
      inventoryPolicy: string; depreciationPolicy: string; userId: string; now: number;
    }) => call('finance_accounting_profile_create', f.id,
      [f.id, f.companyId, f.statutoryChartId, f.standard, f.fiscalStartMonth, f.effectiveDate, f.eligibilityEvidence, f.policyEvidence, f.taxBasis, f.inventoryPolicy, f.depreciationPolicy, f.userId, f.now]),
    decide: (f: { status: string; userId: string; reason: string; now: number; id: string; companyId: string }) =>
      call('finance_accounting_profile_decide', '', [f.status, f.userId, f.reason, f.now, f.id, f.companyId]),
  };
}
