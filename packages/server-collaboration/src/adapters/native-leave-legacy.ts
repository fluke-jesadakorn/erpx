import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import { nativeSystemNotificationStatement } from './native-system-notification';

/** Native-owned legacy leave statements. */
export function nativeLeaveLegacy(db: SqlDatabase) {
  const call = (companyId: string, kind: string, values: unknown[]): Promise<SqlStatement> =>
    nativeSystemNotificationStatement(db, companyId,
      { userId: '', recordType: '', recordId: '', recordVersion: 0, message: '', kind, values_json: JSON.stringify(values) }, '', 0);
  return {
    employee: (companyId: string, values: unknown[]) => call(companyId, 'people_legacy_leave_employee', values),
    request: (companyId: string, values: unknown[]) => call(companyId, 'people_legacy_leave_request', values),
  };
}
