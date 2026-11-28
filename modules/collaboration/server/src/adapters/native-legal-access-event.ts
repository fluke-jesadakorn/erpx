import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/platform-runtime-server/public/runtime/types';
import { nativeSystemNotificationStatement } from './native-system-notification';

/** Native-owned legal document access event. */
export function legalAccessEvent(db: SqlDatabase, companyId: string, userId: string) {
  return {
    record(id: string, action: string, itemId: string, documentId: string, now: number): Promise<SqlStatement> {
      return nativeSystemNotificationStatement(db, companyId,
        { userId, recordType: '', recordId: '', recordVersion: 0, message: '', kind: 'legal_access_event',
          values_json: JSON.stringify([id, itemId, documentId, userId, action, now]) }, id, now);
    },
  };
}
