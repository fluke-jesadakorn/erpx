import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import { nativeSystemNotificationStatement } from './native-system-notification';

/** Native-owned legal portal attachment staging statements. */
export function legalPortalStagedStatements(db: SqlDatabase) {
  const stmt = (companyId: string, input: Parameters<typeof nativeSystemNotificationStatement>[2], key: string, now: number): Promise<SqlStatement> =>
    nativeSystemNotificationStatement(db, companyId, input, key, now);
  return {
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
