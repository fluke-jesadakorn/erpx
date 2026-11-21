import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import { nativeSystemNotificationStatement } from './native-system-notification';

/** Native-owned legal approval statements. */
export function legalApprovalStatements(db: SqlDatabase, companyId: string, userId: string) {
  const call = (kind: string, key: string, values: unknown[]): Promise<SqlStatement> =>
    nativeSystemNotificationStatement(db, companyId,
      { userId, recordType: '', recordId: '', recordVersion: 0, message: '', kind, values_json: JSON.stringify(values) }, key, 0);
  return {
    lockDocument: (f: { id: string }) => call('legal_approval_lock', '', [f.id]),
    retainRoute: (f: {
      itemId: string; instanceId: string; id: string; checksum: string;
      signerIdsJson: string; nextVersion: number; actorId: string; sourceHash: string;
    }) => call('legal_approval_retain_route', '',
      [f.itemId, f.instanceId, f.id, f.checksum, f.signerIdsJson, f.nextVersion, f.actorId, f.sourceHash]),
    recordApproval: (f: {
      itemId: string; nextVersion: number; document_id: string; document_hash: string;
      signerIdsJson: string; actorId: string; now: number;
    }) => call('legal_approval_record', '',
      [f.itemId, f.nextVersion, f.document_id, f.document_hash, f.signerIdsJson, f.actorId, f.now]),
    advanceContract: (f: { state: string; now: number; itemId: string }) =>
      call('legal_approval_advance', '', [f.state, f.now, f.itemId]),
    event: (f: { id: string; action: string; note: string; version: number; identifier: string; now: number }) =>
      call('legal_approval_event', f.identifier, [f.identifier, f.id, f.action, userId, f.note, f.version, f.now]),
  };
}
