import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import { nativeSystemNotificationStatement } from './native-system-notification';

/** Native-owned finance/commerce statement. */
export function financeCommerceFacts(db: SqlDatabase, companyId: string) {
  return {
    baseCarry: (f: { baseDebit: number; baseCredit: number; id: string; ordinal: number; accountId: string }): Promise<SqlStatement> =>
      nativeSystemNotificationStatement(db, companyId,
        { userId: '', recordType: '', recordId: '', recordVersion: 0, message: '', kind: 'finance_commerce_base_carry',
          values_json: JSON.stringify([f.baseDebit, f.baseCredit, f.id, f.ordinal, f.accountId]) }, '', 0),
  };
}
