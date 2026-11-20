import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import { nativeSystemNotificationStatement } from './native-system-notification';

/** Native-owned legal operations statements. */
export function legalOperations(db: SqlDatabase, companyId: string, userId: string) {
  const call = (kind: string, key: string, values: unknown[]): Promise<SqlStatement> =>
    nativeSystemNotificationStatement(db, companyId,
      { userId, recordType: '', recordId: '', recordVersion: 0, message: '', kind, values_json: JSON.stringify(values) }, key, 0);
  return {
    async accessEvent(f: { identifier: string; id: string; actorId: string; action: string; now: number }) {
      return call('legal_access_event', f.identifier, [f.identifier, f.id, null, f.actorId, f.action, f.now]);
    },
    saveView: (f: { companyId: string; actorId: string; name: string; queryJson: string }) =>
      call('legal_saved_view', '', [f.companyId, f.actorId, f.name, f.queryJson]),
    async derivedLink(f: { id: string; id2: string }) {
      return (await call('legal_derived_link', '', [f.id, f.id2])).run();
    },
    aiDocument: (f: {
      documentId: string; itemId: string; version: number; name: string;
      checksum: string; bytes: number; body: string; actorId: string; now: number;
    }) => call('legal_ai_document', f.documentId,
      [f.documentId, f.itemId, f.version, f.name, f.checksum, f.bytes, f.body, f.actorId, f.now]),
    acceptProposal: (f: { proposalJson: string; proposalId: string }) =>
      call('legal_proposal_accept', '', [f.proposalJson, f.proposalId]),
    advanceItem: (f: { now: number; id: string }) =>
      call('legal_advance_record', '', [f.now, f.id]),
  };
}
