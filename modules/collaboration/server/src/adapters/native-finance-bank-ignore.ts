import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/platform-runtime-server/public/runtime/types';
import { nativeSystemNotificationStatement } from './native-system-notification';

/** Native-owned payroll bank observation ignore statement. */
export function financeBankIgnore(db: SqlDatabase) {
  return {
    ignore(facts: { bankTransactionId: string; companyId: string; bankVersion: number }): Promise<SqlStatement> {
      return nativeSystemNotificationStatement(db, facts.companyId,
        { userId: '', recordType: '', recordId: facts.bankTransactionId, recordVersion: facts.bankVersion, message: '', kind: 'finance_bank_ignore' }, '', 0);
    },
  };
}
