import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import { nativeSystemNotificationStatement } from './native-system-notification';

/** Native-owned legal trust and processing statements. */
export function legalTrustProcessing(db: SqlDatabase, companyId: string, userId: string) {
  const call = (kind: string, key: string, values: unknown[]): Promise<SqlStatement> =>
    nativeSystemNotificationStatement(db, companyId,
      { userId, recordType: '', recordId: '', recordVersion: 0, message: '', kind, values_json: JSON.stringify(values) }, key, 0);
  return {
    addition: (f: { key: string; kind: string; name: string; source: string; content: string; checksum: string; now: number }) =>
      call('legal_trust_addition', f.key, [f.key, companyId, f.kind, f.name, f.source, f.content, f.checksum, userId, f.now]),
    decision: (f: { key: string; state: string; id: string; version: number }) =>
      call('legal_trust_decision', f.key, [f.state, userId, f.id]),
    job: (f: {
      key: string; itemId: string; documentId: string; checksum: string;
      kind: string; actorJson: string; created: number;
    }) => call('legal_processing_job', f.key,
      [f.key, companyId, f.itemId, f.documentId, f.checksum, f.kind, f.actorJson, userId, f.created]),
  };
}
