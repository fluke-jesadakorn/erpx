import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import { nativeSystemNotificationStatement } from './native-system-notification';

/** Native-owned finance/payroll owner statements. */
export function financePayrollOwner(db: SqlDatabase, companyId: string) {
  const call = (kind: string, values: unknown[]): Promise<SqlStatement> =>
    nativeSystemNotificationStatement(db, companyId,
      { userId: '', recordType: '', recordId: '', recordVersion: 0, message: '', kind, values_json: JSON.stringify(values) }, '', 0);
  return {
    invoiceVersion: (f: { now: number; sourceId: string; companyId: string }) => call('finance_payroll_batch_invoice_version', [f.now, f.sourceId, f.companyId]),
    bankMatch: (f: { id: string; id2: string; bankVersion: number }) => call('finance_payroll_batch_bank_match', [f.id, f.id2, f.bankVersion]),
  };
}
