import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import { nativeSystemNotificationStatement } from './native-system-notification';

/** Native-owned legal backfill attachment statements. */
export function nativeBackfillAttachment(db: SqlDatabase, companyId: string) {
  const call = (kind: string, values: unknown[]): Promise<SqlStatement> =>
    nativeSystemNotificationStatement(db, companyId,
      { userId: '', recordType: '', recordId: '', recordVersion: 0, message: '', kind, values_json: JSON.stringify(values) }, '', 0);
  return {
    legalDocument: (linkId: string, attachmentId: string, id: string) => call('legal_backfill_document_attachment', [linkId, attachmentId, id]),
    legalExternal: (linkId: string, attachmentId: string, id: string) => call('legal_backfill_external_attachment', [linkId, attachmentId, id]),
  };
}
