import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import { nativeSystemNotificationStatement } from './native-system-notification';

/** Native-owned legal feature statements. */
export function legalFeatureStatements(db: SqlDatabase, companyId: string, userId: string) {
  const call = (kind: string, key: string, values: unknown[], table?: string): Promise<SqlStatement> =>
    nativeSystemNotificationStatement(db, companyId,
      { userId, recordType: '', recordId: '', recordVersion: 0, message: '', kind, values_json: JSON.stringify(values), table_name: table }, key, 0);
  return {
    createFeature(f: {
      id: string; itemId: string; category: string; title: string; owner_id: string;
      due_date: string | null; state: string; dataJson: string; actorId: string;
      actorId2: string; updated_at: number; table: string;
    }) {
      return call('legal_feature_create', f.id,
        [f.id, f.itemId, f.category, f.title, f.owner_id, f.due_date, f.state, f.dataJson, f.actorId, f.actorId2, f.updated_at], f.table);
    },
    updateFeature(f: {
      state: string; dataJson: string; actorId: string; updated_at: number;
      id: string; version: number; table: string;
    }) {
      return call('legal_feature_update', f.id,
        [f.state, f.dataJson, f.actorId, f.updated_at, f.id, f.version], f.table);
    },
    recordFeatureHistory(f: {
      identifier: string; itemId: string; id: string; category: string; version: number;
      snapshotJson: string; actorId: string; reason: string; now: number;
    }) {
      return call('legal_feature_history', f.identifier,
        [f.identifier, f.itemId, f.id, f.category, f.version, f.snapshotJson, f.actorId, f.reason, f.now]);
    },
    advanceRecord(f: { now: number; itemId: string }) {
      return call('legal_advance_record', '', [f.now, f.itemId]);
    },
    event(f: { id: string; action: string; note: string; version: number; identifier: string; now: number }) {
      return call('legal_feature_event', f.identifier, [f.identifier, f.id, f.action, userId, f.note, f.version, f.now]);
    },
  };
}
