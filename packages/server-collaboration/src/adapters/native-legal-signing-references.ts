import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import { nativeSystemNotificationStatement } from './native-system-notification';

/** Native-owned legal signing and reference statements. */
export function legalSigningReferences(db: SqlDatabase, companyId: string, userId: string) {
  const call = (kind: string, key: string, values: unknown[]): Promise<SqlStatement> =>
    nativeSystemNotificationStatement(db, companyId,
      { userId, recordType: '', recordId: '', recordVersion: 0, message: '', kind, values_json: JSON.stringify(values) }, key, 0);
  return {
    signerView: (f: { viewed: number; envelopeId: string }) => call('legal_signer_view', '', [f.viewed, f.envelopeId, userId]),
    itemStateAdvance: (f: { state: string; updated: number; itemId: string }) => call('legal_item_state_advance', '', [f.state, f.updated, f.itemId]),
    itemAdvance: (f: { updated: number; itemId: string }) => call('legal_advance_record', '', [f.updated, f.itemId]),
    addReference: (f: { itemId: string; targetKind: string; targetId: string; createdBy: string; createdAt: number }) =>
      call('legal_native_reference', '', [f.itemId, f.targetKind, f.targetId, f.itemId, f.createdBy, f.createdAt]),
    event: (f: { id: string; action: string; note: string; version: number; identifier: string; now: number }) =>
      call('legal_approval_event', f.identifier, [f.identifier, f.id, f.action, userId, f.note, f.version, f.now]),
  };
}
