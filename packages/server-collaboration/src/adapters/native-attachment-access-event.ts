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

/** Native-owned POS attachment content-read audit event. */
export function writeAttachmentEvents1(
  db: import('@erpx/server-runtime/public/runtime/types').SqlDatabase,
  facts: {
    fact0: string; companyId: string; attachment_source_id: string;
    attachment_link_id: string; attachment_set_id: string; userId: string;
    fact6: string; fact7: number;
  },
): Promise<import('@erpx/server-runtime/public/runtime/types').SqlStatement> {
  return nativeSystemNotificationStatement(db, facts.companyId,
    {
      userId: facts.userId, recordType: 'attachment_content_read', recordId: facts.attachment_link_id,
      recordVersion: 1, message: facts.fact6, kind: 'attachment_event',
      identifier: facts.attachment_source_id, item_id: facts.attachment_set_id,
    },
    facts.fact0, facts.fact7,
  );
}
