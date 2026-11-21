import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import { nativeSystemNotificationStatement } from './native-system-notification';

/** Native-owned finance/payroll remittance statement. */
export function financePayrollRemittance(db: SqlDatabase, companyId: string) {
  return {
    matchBank: (f: { remittanceId: string; bankTransactionId: string; bankVersion: number }): Promise<SqlStatement> =>
      nativeSystemNotificationStatement(db, companyId,
        { userId: '', recordType: '', recordId: '', recordVersion: 0, message: '', kind: 'finance_payroll_remittance_match',
          values_json: JSON.stringify([f.remittanceId, f.bankTransactionId, f.bankVersion]) }, '', 0),
  };
}
