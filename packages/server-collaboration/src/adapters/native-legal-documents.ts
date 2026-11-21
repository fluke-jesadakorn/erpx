import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import { nativeSystemNotificationStatement } from './native-system-notification';

/** Native-owned legal document statements. */
export function legalDocuments(db: SqlDatabase, companyId: string, userId: string) {
  const call = (kind: string, key: string, values: unknown[]): Promise<SqlStatement> =>
    nativeSystemNotificationStatement(db, companyId,
      { userId, recordType: '', recordId: '', recordVersion: 0, message: '', kind, values_json: JSON.stringify(values) }, key, 0);
  return {
    inspection: (f: { inspectionId: string; id: string; kind: string; resultJson: string; actorId: string; now: number; itemId: string }) =>
      call('legal_document_inspection', f.inspectionId,
        [f.inspectionId, f.id, f.kind, f.resultJson, f.actorId, f.now, f.itemId]),
    retainExecutedCopy: (f: {
      executedCopyId: string; id: string; documentId: string; cycle: number;
      document_id: string; document_hash: string; checksum: string;
      metadataJson: string; actorId: string; now: number;
    }) => call('legal_executed_copy_retain', f.executedCopyId,
      [f.executedCopyId, f.id, f.documentId, f.cycle, f.document_id, f.document_hash, f.checksum, f.metadataJson, f.actorId, f.now]),
    lockExecutedDocument: (f: { documentId: string }) => call('legal_approval_lock', '', [f.documentId]),
    insertCanonicalDocument: (f: {
      documentId: string; id: string; next: number; name: string; media_type: string;
      checksum_sha256: string; fact6: number; object_key: string; actorId: string;
      now: number; attachmentLinkId: string; sourceId: string;
    }) => call('legal_canonical_document_insert', f.documentId,
      [f.documentId, f.id, f.next, f.name, f.media_type, f.checksum_sha256, f.fact6, f.object_key, f.actorId, f.now, f.attachmentLinkId, f.sourceId]),
    advanceCanonicalItem: (f: { now: number; id: string }) => call('legal_advance_record', '', [f.now, f.id]),
    insertStoredDocument: (f: {
      identifier: string; id: string; next: number; name: string; type: string;
      checksum: string; length: number; objectKey: string; actorId: string; now: number;
    }) => call('legal_stored_document_insert', f.identifier,
      [f.identifier, f.id, f.next, f.name, f.type, f.checksum, f.length, f.objectKey, f.actorId, f.now]),
    advanceStoredItem: (f: { now: number; id: string }) => call('legal_advance_record', '', [f.now, f.id]),
    accessEvent: (f: { identifier: string; itemId: string; id: string | null; actorId: string; action: string; now: number }) =>
      call('legal_access_event', f.identifier, [f.identifier, f.itemId, f.id, f.actorId, f.action, f.now]),
    event: (f: { id: string; action: string; note: string; version: number; identifier: string; now: number }) =>
      call('legal_approval_event', f.identifier, [f.identifier, f.id, f.action, userId, f.note, f.version, f.now]),
  };
}
