import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import { nativeSystemNotificationStatement } from './native-system-notification';

/** Native-owned legal executed-copy review statements. */
export function legalExecutedCopies(db: SqlDatabase, companyId: string, userId: string) {
  const call = (kind: string, key: string, values: unknown[]): Promise<SqlStatement> =>
    nativeSystemNotificationStatement(db, companyId,
      { userId, recordType: '', recordId: '', recordVersion: 0, message: '', kind, values_json: JSON.stringify(values) }, key, 0);
  return {
    review: (f: {
      key: string; executedCopyId: string; itemVersion: number; decision: string;
      checks: string; reason: string; now: number;
    }) => call('legal_executed_review', f.key,
      [f.key, f.executedCopyId, f.itemVersion, f.decision, f.checks, f.reason, userId, f.now]),
    copyState: (f: { state: string; reviewedBy: string | null; reviewedAt: number | null; id: string }) =>
      call('legal_executed_copy_state', '', [f.state, f.reviewedBy, f.reviewedAt, f.id]),
    advanceContract: (f: { state: string; now: number; itemId: string }) =>
      call('legal_approval_advance', '', [f.state, f.now, f.itemId]),
    event: (f: { id: string; action: string; note: string; version: number; identifier: string; now: number }) =>
      call('legal_approval_event', f.identifier, [f.identifier, f.id, f.action, userId, f.note, f.version, f.now]),
  };
}
