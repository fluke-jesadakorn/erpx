import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import { nativeSystemNotificationStatement } from './native-system-notification';

/** Native-owned people/payroll expense draft statement. */
export function expenseDrafts(db: SqlDatabase, companyId: string) {
  return {
    write(isInsert: boolean, values: unknown[], recordId: string, userId: string, now: number): Promise<SqlStatement> {
      const binds = isInsert ? [...values, recordId, companyId, userId, now] : [...values, recordId];
      return nativeSystemNotificationStatement(db, companyId,
        { userId, recordType: '', recordId: '', recordVersion: 0, message: '', kind: 'expense_draft_write',
          values_json: JSON.stringify(binds), identifier: isInsert ? 'insert' : 'update' }, recordId, now);
    },
  };
}
