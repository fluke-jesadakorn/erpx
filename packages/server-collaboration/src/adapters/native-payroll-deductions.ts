import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import { nativeSystemNotificationStatement } from './native-system-notification';

/** Native-owned people/payroll advance deduction statements. */
export function payrollDeductions(db: SqlDatabase, companyId: string) {
  const call = (kind: string, key: string, values: unknown[]): Promise<SqlStatement> =>
    nativeSystemNotificationStatement(db, companyId,
      { userId: '', recordType: '', recordId: '', recordVersion: 0, message: '', kind, values_json: JSON.stringify(values) }, key, 0);
  return {
    consumption: (f: {
      id: string; companyId: string; repaymentId: string; sourceVersion: number; runId: string;
      itemId: string; journalId: string; receivableAccountId: string; amount: number; key: string; now: number;
    }) => call('payroll_deduction_consumption', '',
      [f.id, f.companyId, f.repaymentId, f.sourceVersion, f.runId, f.itemId, f.journalId, f.receivableAccountId, f.amount, f.key, f.now]),
    collect: (f: { runId: string; repaymentId: string; companyId: string; version: number }) =>
      call('payroll_repayment_collect', '', [f.runId, f.repaymentId, f.companyId, f.version]),
    close: (f: { now: number; companyId: string; runId: string }) =>
      call('payroll_advances_close', '', [f.now, f.companyId, f.runId]),
  };
}
