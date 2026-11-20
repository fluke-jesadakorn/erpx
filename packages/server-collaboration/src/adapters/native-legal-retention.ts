import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import { nativeSystemNotificationStatement } from './native-system-notification';

/** Native-owned legal retention statements. */
export function legalRetention(db: SqlDatabase, companyId: string, userId: string) {
  const call = (kind: string, key: string, values: unknown[]): Promise<SqlStatement> =>
    nativeSystemNotificationStatement(db, companyId,
      { userId, recordType: '', recordId: '', recordVersion: 0, message: '', kind, values_json: JSON.stringify(values) }, key, 0);
  return {
    createPolicy: (f: { key: string; companyId: string; name: string; days: number; reason: string; actorId: string; now: number }) =>
      call('legal_retention_policy_create', f.key, [f.key, f.companyId, f.name, f.days, f.reason, f.actorId, f.now]),
    decidePolicy: (f: { fact0: string; actorId: string; id: string }) =>
      call('legal_retention_policy_decide', '', [f.fact0, f.actorId, f.id]),
    assignRetention: (f: {
      id: string; id2: string; version: number; updated_at: number; timezone: string;
      days: number; timezone2: string; actorId: string; now: number;
    }) => call('legal_retention_assign', '',
      [f.id, f.id2, f.version, f.updated_at, f.timezone, f.days, f.timezone2, f.actorId, f.now]),
    requestDestruction: (f: {
      key: string; id: string; policy_id: string; policy_version: number;
      actorId: string; reason: string; manifestJson: string; now: number;
    }) => call('legal_retention_request', f.key,
      [f.key, f.id, f.policy_id, f.policy_version, f.actorId, f.reason, f.manifestJson, f.now]),
    changeDestruction: (f: { state: string; fact1: string | null; fact2: number | null; id: string }) =>
      call('legal_retention_change', '', [f.state, f.fact1, f.fact2, f.id]),
    advanceRecord: (f: { now: number; itemId: string }) =>
      call('legal_advance_record', '', [f.now, f.itemId]),
    event: (f: { id: string; action: string; note: string; version: number; identifier: string; now: number }) =>
      call('legal_approval_event', f.identifier, [f.identifier, f.id, f.action, userId, f.note, f.version, f.now]),
  };
}
