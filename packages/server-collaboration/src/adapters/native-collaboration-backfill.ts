import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import { nativeSystemNotificationStatement } from './native-system-notification';

/** Native-owned collaboration backfill statements. */
export function nativeCollaborationBackfill(db: SqlDatabase, companyId: string) {
  return {
    messagingAttachment: (setId: string, linkId: string, attachmentId: string, id: string): Promise<SqlStatement> =>
      nativeSystemNotificationStatement(db, companyId,
        { userId: '', recordType: '', recordId: '', recordVersion: 0, message: '',
          kind: 'collaboration_backfill_messaging_attachment', values_json: JSON.stringify([setId, linkId, attachmentId, id]) }, '', 0),
  };
}
