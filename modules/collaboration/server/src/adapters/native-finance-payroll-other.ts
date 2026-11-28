import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/platform-runtime-server/public/runtime/types';
import { nativeSystemNotificationStatement } from './native-system-notification';

/** Native-owned finance/payroll "other" remittance statement. */
export function financePayrollOther(db: SqlDatabase, companyId: string) {
  return {
    matchBank: (f: { remittanceId: string; bankTransactionId: string; bankVersion: number; companyId: string }): Promise<SqlStatement> =>
      nativeSystemNotificationStatement(db, companyId,
        { userId: '', recordType: '', recordId: '', recordVersion: 0, message: '', kind: 'finance_payroll_other_match',
          values_json: JSON.stringify([f.remittanceId, f.bankTransactionId, f.companyId, f.bankVersion]) }, '', 0),
  };
}
