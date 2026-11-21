import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import { nativeSystemNotificationStatement } from './native-system-notification';

/** Native-owned legal destruction worker statements. */
export function legalDestructionWorker(db: SqlDatabase, companyId: string) {
  const call = (kind: string, key: string, values: unknown[]): Promise<SqlStatement> =>
    nativeSystemNotificationStatement(db, companyId,
      { userId: '', recordType: '', recordId: '', recordVersion: 0, message: '', kind, values_json: JSON.stringify(values) }, key, 0);
  return {
    block: (f: { id: string }) => call('legal_destruction_block', '', [f.id]),
    start: (f: { now: number; id: string }) => call('legal_destruction_start', '', [f.now, f.id]),
    startedEvent: (f: { identifier: string; fact1: string; now: number; item_id: string }) =>
      call('legal_retention_started_event', f.identifier, [f.identifier, f.fact1, f.now, f.item_id]),
    countAttempt: (f: { id: string }) => call('legal_destruction_count_attempt', '', [f.id]),
    retainExecuting: (f: { id: string }) => call('legal_destruction_retain_executing', '', [f.id]),
    retainDisposal: (f: { id: string; id2: string; checksum: string; now: number }) =>
      call('legal_disposal_retain', '', [f.id, f.id2, f.checksum, f.now]),
    eraseDocument: (f: { id: string }) => call('legal_document_erase', '', [f.id]),
    retireProcessing: (f: { item_id: string }) => call('legal_processing_retire', '', [f.item_id]),
    eraseInspections: (f: { item_id: string }) => call('legal_inspections_erase', '', [f.item_id]),
    retireAi: (f: { item_id: string }) => call('legal_ai_retire', '', [f.item_id]),
    revokeShares: (f: { now: number; item_id: string }) => call('legal_shares_revoke', '', [f.now, f.item_id]),
    complete: (f: { now: number; id: string }) => call('legal_destruction_complete', '', [f.now, f.id]),
    completedEvent: (f: { identifier: string; item_id: string; fact2: string; item_version: number; now: number }) =>
      call('legal_retention_destroyed_event', f.identifier, [f.identifier, f.item_id, f.fact2, f.item_version, f.now]),
    requireReview: (f: { id: string }) => call('legal_destruction_require_review', '', [f.id]),
  };
}
