import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import { nativeSystemNotificationStatement } from './native-system-notification';

/** Native-owned legal item lifecycle event. */
export function legalEvent(db: SqlDatabase, companyId: string, userId: string) {
  return {
    record(id: string, action: string, note: string, version: number, key: string, now: number): Promise<SqlStatement> {
      return nativeSystemNotificationStatement(db, companyId,
        { userId, recordType: '', recordId: id, recordVersion: version, message: note, kind: 'legal_approval_event' }, key, now);
    },
  };
}
