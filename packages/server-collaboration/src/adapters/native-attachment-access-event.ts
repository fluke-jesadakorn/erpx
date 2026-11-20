import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import { nativeSystemNotificationStatement } from './native-system-notification';

/** Native-owned customer portal attachment preview access audit event. */
export function recordCustomerPortalFileAccess(
  db: SqlDatabase,
  event: { id: string; companyId: string; attachmentId: string; linkId: string; userId: string; now: number },
): Promise<SqlStatement> {
  return nativeSystemNotificationStatement(db, event.companyId,
    {
      userId: event.userId,
      recordType: 'attachment_preview',
      recordId: event.linkId,
      recordVersion: 1,
      message: '',
      kind: 'attachment_access_event',
      identifier: event.attachmentId,
    },
    event.id, event.now,
  );
}


/** Support-ticket portal variant (same owner event shape). */
export const supportTicketPortalFileAccess = recordCustomerPortalFileAccess;
