import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import { nativeSystemNotificationStatement } from './native-system-notification';

/** Native-owned legal portal attachment binding statements. */
export function legalPortalBindingStatements(db: SqlDatabase) {
  return {
    bindLink(facts: { now: number; key: string; linkId: string; sourceId: string; companyId: string }): Promise<SqlStatement> {
      return nativeSystemNotificationStatement(db, facts.companyId,
        { userId: '', recordType: '', recordId: facts.linkId, recordVersion: 1, message: '', kind: 'legal_bind_link', identifier: facts.sourceId }, facts.key, facts.now);
    },
    bindSet(facts: { now: number; key: string; now2: number; setId: string; expectedTargetVersion: number; companyId: string }): Promise<SqlStatement> {
      return nativeSystemNotificationStatement(db, facts.companyId,
        { userId: '', recordType: '', recordId: facts.setId, recordVersion: facts.expectedTargetVersion, message: '', kind: 'legal_bind_set' }, facts.key, facts.now);
    },
    bindEvent(facts: { fact0: string; company_id: string; sourceId: string; linkId: string; setId: string; actorId: string; fact6: 'set.bound'; fact7: string; now: number }): Promise<SqlStatement> {
      return nativeSystemNotificationStatement(db, facts.company_id,
        { userId: facts.actorId, recordType: facts.fact6, recordId: facts.linkId, recordVersion: 1, message: facts.fact7, kind: 'legal_bind_event', identifier: facts.sourceId, item_id: facts.setId }, facts.fact0, facts.now);
    },
  };
}
