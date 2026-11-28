import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/platform-runtime-server/public/runtime/types';
import { nativeSystemNotificationStatement } from './native-system-notification';

/** Native-owned legal portal attachment staging statements. */
export function legalPortalStagedStatements(db: SqlDatabase) {
  const stmt = (companyId: string, input: Parameters<typeof nativeSystemNotificationStatement>[2], key: string, now: number): Promise<SqlStatement> =>
    nativeSystemNotificationStatement(db, companyId, input, key, now);
  return {

    draftSet(facts: {
      setId: string; companyId: string; branchId: string | null; departmentId: string | null;
      fact4: string; itemId: string; fact6: string; commandId: string; PORTAL_PURPOSE: string;
      itemVersion: number; version: number; classification: string; fact12: string;
      retentionPolicyKey: string | null; requiredCount: number; maxFiles: number;
      actorId: string; now: number; now2: number;
    }) {
      const values = [facts.setId, facts.companyId, facts.branchId, facts.departmentId, facts.fact4, facts.itemId, facts.fact6, facts.commandId, facts.PORTAL_PURPOSE, facts.itemVersion, facts.version, facts.classification, facts.fact12, facts.retentionPolicyKey, facts.requiredCount, facts.maxFiles, facts.actorId, facts.now, facts.now2];
      return stmt(facts.companyId, { userId: '', recordType: '', recordId: '', recordVersion: 0, message: '', kind: 'legal_draft_set', values_json: JSON.stringify(values) }, '', 0);
    },
    draftLink(facts: { linkId: string; setId: string; actorId: string; now: number; fact4: number; companyId: string }) {
      const values = [facts.linkId, facts.setId, facts.actorId, facts.now, facts.fact4];
      return stmt(facts.companyId, { userId: '', recordType: '', recordId: '', recordVersion: 0, message: '', kind: 'legal_draft_link', values_json: JSON.stringify(values) }, '', 0);
    },
    uploadedIntent(facts: {
      uploadId: string; fact1: string; itemId: string; PORTAL_PURPOSE: string; name: string;
      mediaType: string; byteLength: number; byteLength2: number; checksum: string; objectKey: string;
      objectKey2: string; actorId: string; fact12: number; now: number; now2: number; itemVersion: number;
      setId: string; linkId: string; companyId: string; mediaType2: string; objectKey3: string;
    }) {
      const values = [facts.uploadId, facts.fact1, facts.itemId, facts.PORTAL_PURPOSE, facts.name, facts.mediaType, facts.byteLength, facts.byteLength2, facts.checksum, facts.objectKey, facts.objectKey2, facts.actorId, facts.fact12, facts.now, facts.now2, facts.itemVersion, facts.setId, facts.linkId, facts.companyId, facts.mediaType2, facts.objectKey3];
      return stmt(facts.companyId, { userId: '', recordType: '', recordId: '', recordVersion: 0, message: '', kind: 'legal_uploaded_intent', values_json: JSON.stringify(values) }, '', 0);
    },
    quarantinedSource(facts: {
      sourceId: string; uploadId: string; fact2: string; itemId: string; PORTAL_PURPOSE: string;
      objectKey: string; name: string; mediaType: string; byteLength: number; checksum: string;
      actorId: string; now: number; companyId: string; mediaType2: string;
    }) {
      const values = [facts.sourceId, facts.uploadId, facts.fact2, facts.itemId, facts.PORTAL_PURPOSE, facts.objectKey, facts.name, facts.mediaType, facts.byteLength, facts.checksum, facts.actorId, facts.now, facts.companyId, facts.mediaType2];
      return stmt(facts.companyId, { userId: '', recordType: '', recordId: '', recordVersion: 0, message: '', kind: 'legal_quarantined_source', values_json: JSON.stringify(values) }, '', 0);
    },
    linkUpload(facts: { uploadId: string; sourceId: string; linkId: string; companyId: string }) {
      return stmt(facts.companyId, { userId: '', recordType: '', recordId: facts.linkId, recordVersion: 0, message: '', kind: 'legal_link_upload', identifier: facts.uploadId, item_id: facts.sourceId }, '', 0);
    },
    pendingScan(facts: { fact0: string; sourceId: string; now: number; companyId: string }) {
      return stmt(facts.companyId, { userId: '', recordType: '', recordId: '', recordVersion: 0, message: '', kind: 'legal_pending_scan', identifier: facts.sourceId }, facts.fact0, facts.now);
    },
    storedOriginal(facts: { fact0: string; companyId: string; sourceId: string; objectKey: string; byteLength: number; now: number }) {
      return stmt(facts.companyId, { userId: '', recordType: '', recordId: '', recordVersion: 0, message: '', kind: 'legal_stored_original', identifier: facts.sourceId, item_id: facts.objectKey, timestamp: facts.byteLength }, facts.fact0, facts.now);
    },
    quarantineEvent(facts: { fact0: string; companyId: string; sourceId: string; linkId: string; setId: string; actorId: string; fact6: string; fact7: string; now: number }) {
      return stmt(facts.companyId, { userId: facts.actorId, recordType: facts.fact6, recordId: facts.linkId, recordVersion: 0, message: facts.fact7, kind: 'legal_quarantine_event', identifier: facts.sourceId, item_id: facts.setId }, facts.fact0, facts.now);
    },
  };
}
