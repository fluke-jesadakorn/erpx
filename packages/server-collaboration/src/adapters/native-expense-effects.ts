import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import { nativeSystemNotificationStatement } from './native-system-notification';

/** Native-owned expense attachment withdrawal/revocation statements. */
export function expenseNativeEffects(db: SqlDatabase) {
  return {
    withdrawLink(facts: { at: number; actorId: string; reason: string; linkId: string; companyId: string }): Promise<SqlStatement> {
      return nativeSystemNotificationStatement(db, facts.companyId,
        { userId: facts.actorId, recordType: '', recordId: facts.linkId, recordVersion: 1, message: facts.reason, kind: 'expense_withdraw_link' }, '', facts.at);
    },
    withdrawEvent(facts: { id: string; companyId: string; attachmentId: string; linkId: string; setId: string; actorId: string; details: string; at: number }): Promise<SqlStatement> {
      return nativeSystemNotificationStatement(db, facts.companyId,
        { userId: facts.actorId, recordType: '', recordId: facts.linkId, recordVersion: 1, message: facts.details, kind: 'expense_withdraw_event', identifier: facts.attachmentId, item_id: facts.setId }, facts.id, facts.at);
    },
    revokeAttachment(facts: { at: number; attachmentId: string; companyId: string }): Promise<SqlStatement> {
      return nativeSystemNotificationStatement(db, facts.companyId,
        { userId: '', recordType: '', recordId: facts.attachmentId, recordVersion: 1, message: '', kind: 'expense_revoke_attachment' }, '', facts.at);
    },
  };
}
