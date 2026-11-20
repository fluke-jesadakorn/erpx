import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import { nativeSystemNotificationStatement } from './native-system-notification';

/** Native-owned document derivative statements. */
export function nativeDocumentDerivativeStatements(db: SqlDatabase, userId: string) {
  return {
    document: (id: string, company: string, checksum: string, pages: number, now: number): Promise<SqlStatement> =>
      nativeSystemNotificationStatement(db, company,
        { userId, recordType: '', recordId: id, recordVersion: pages, message: checksum, kind: 'doc_attachment_record' }, checksum, now),
    derivative: (id: string, attachment: string, company: string, page: number, key: string, checksum: string, original: string, bytes: number, user: string, now: number): Promise<SqlStatement> =>
      nativeSystemNotificationStatement(db, company,
        { userId, recordType: '', recordId: attachment, recordVersion: page, message: checksum, kind: 'doc_derivative', identifier: key, item_id: original, timestamp: bytes }, id, now),
    storage: (id: string, company: string, attachment: string, key: string, bytes: number, now: number): Promise<SqlStatement> =>
      nativeSystemNotificationStatement(db, company,
        { userId, recordType: '', recordId: attachment, recordVersion: 0, message: '', kind: 'doc_storage_ledger', identifier: key, timestamp: bytes }, id, now),
  };
}
