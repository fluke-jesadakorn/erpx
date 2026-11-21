import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import { nativeSystemNotificationStatement } from './native-system-notification';

/** Native-owned finance intercompany statements. */
export function nativeFinanceIntercompany(db: SqlDatabase, companyId: string) {
  const call = (kind: string, values: unknown[]): Promise<SqlStatement> =>
    nativeSystemNotificationStatement(db, companyId,
      { userId: '', recordType: '', recordId: '', recordVersion: 0, message: '', kind, values_json: JSON.stringify(values) }, '', 0);
  return {
    groupEntity: (v: unknown[]) => call('finance_group_entity', v),
    transaction: (v: unknown[]) => call('finance_intercompany_transaction', v),
    event: (v: unknown[]) => call('finance_intercompany_event', v),
    status: (v: unknown[]) => call('finance_intercompany_status', v),
  };
}
