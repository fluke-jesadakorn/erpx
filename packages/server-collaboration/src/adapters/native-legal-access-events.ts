import 'server-only';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import { nativeSystemNotificationStatement } from './native-system-notification';

/** Native-owned legal portal attachment access-event statements. */
export function legalAccessEvents(db: SqlDatabase) {
  return {
    async allowedRead(facts: { fact0: string; companyId: string; attachment_id: string; link_id: string; actorId: string; now: number }) {
      const stmt = await nativeSystemNotificationStatement(db, facts.companyId,
        { userId: facts.actorId, recordType: '', recordId: facts.link_id, recordVersion: 0, message: '', kind: 'legal_access_allowed', identifier: facts.attachment_id }, facts.fact0, facts.now);
      return stmt.run();
    },
    async deniedRead(facts: { fact0: string; companyId: string; attachment_id: string; link_id: string; actorId: string; fact5: string; now: number }) {
      const stmt = await nativeSystemNotificationStatement(db, facts.companyId,
        { userId: facts.actorId, recordType: '', recordId: facts.link_id, recordVersion: 0, message: facts.fact5, kind: 'legal_access_denied', identifier: facts.attachment_id }, facts.fact0, facts.now);
      return stmt.run();
    },
  };
}
