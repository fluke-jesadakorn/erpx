import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import { nativeSystemNotificationStatement } from './native-system-notification';

/** Native-owned legal portal write statements. */
export function legalPortalWrites(db: SqlDatabase) {
  const call = (companyId: string, kind: string, key: string, values: unknown[]): Promise<SqlStatement> =>
    nativeSystemNotificationStatement(db, companyId,
      { userId: '', recordType: '', recordId: '', recordVersion: 0, message: '', kind, values_json: JSON.stringify(values) }, key, 0);
  return {
    accessEvent: (f: { companyId: string; identifier: string; item_id: string; document_id: string; actorId: string; action: string; now: number }) =>
      call(f.companyId, 'legal_portal_access_event', f.identifier, [f.identifier, f.item_id, f.document_id, f.actorId, f.action, f.now]),
    acceptShare: (f: { companyId: string; actorId: string; id: string }) =>
      call(f.companyId, 'legal_portal_accept_share', '', [f.actorId, f.id]),
    externalComment: (f: { companyId: string; identifier: string; item_id: string; document_id: string; body: string; actorId: string; now: number }) =>
      call(f.companyId, 'legal_portal_external_comment', f.identifier, [f.identifier, f.item_id, f.document_id, f.body, f.actorId, f.now]),
    counselEntry: (f: { companyId: string; key: string; item_id: string; title: string; actorId: string; fact4: string; actorId2: string; actorId3: string; now: number }) =>
      call(f.companyId, 'legal_portal_counsel_entry', f.key, [f.key, f.item_id, f.title, f.actorId, f.fact4, f.actorId2, f.actorId3, f.now]),
    counselHistory: (f: { companyId: string; identifier: string; item_id: string; key: string; fact3: string; actorId: string; now: number }) =>
      call(f.companyId, 'legal_portal_counsel_history', f.identifier, [f.identifier, f.item_id, f.key, f.fact3, f.actorId, f.now]),
    advanceItem: (f: { companyId: string; now: number; item_id: string }) =>
      call(f.companyId, 'legal_advance_record', '', [f.now, f.item_id]),
    viewEnvelope: (f: { companyId: string; now: number; id: string; actorId: string }) =>
      call(f.companyId, 'legal_signer_view', '', [f.now, f.id, f.actorId]),
    interactionEvent: (f: { companyId: string; identifier: string; fact1: string; actorId: string; document_id: string; now: number; item_id: string }) =>
      call(f.companyId, 'legal_portal_interaction_event', f.identifier, [f.identifier, f.fact1, f.actorId, f.document_id, f.now, f.item_id]),
    receipt: (f: { companyId: string; key: string; actorId: string; digest: string; fact3: string; now: number }) =>
      call(f.companyId, 'legal_portal_receipt', f.key, [f.key, f.actorId, f.digest, f.fact3, f.now]),
    externalUpload: (f: { companyId: string; key: string; shareId: string; objectKey: string; checksum: string; name: string; mediaType: string; bytes: number; actorId: string; now: number; linkId: string; sourceId: string }) =>
      call(f.companyId, 'legal_portal_external_upload', f.key, [f.key, f.shareId, f.objectKey, f.checksum, f.name, f.mediaType, f.bytes, f.actorId, f.now, f.linkId, f.sourceId]),
    advanceUploadItem: (f: { companyId: string; now: number; item_id: string; expectedTargetVersion: number }) =>
      call(f.companyId, 'legal_portal_advance_upload', '', [f.now, f.item_id, f.companyId, f.expectedTargetVersion]),
    uploadEvent: (f: { companyId: string; identifier: string; actorId: string; fact2: string; now: number; item_id: string }) =>
      call(f.companyId, 'legal_portal_upload_event', f.identifier, [f.identifier, f.actorId, f.fact2, f.now, f.item_id]),
  };
}
