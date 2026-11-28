import 'server-only';
import type { SqlDatabase } from '@erpx/platform-runtime-server/public/runtime/types';
import { nativeSystemNotificationStatement } from './native-system-notification';

/** Native-owned customer portal action statement. */
export function customerPortalActions(db: SqlDatabase) {
  return {
    async record(facts: {
      id: string; companyId: string; bindingId: string; orderId: string; customerId: string;
      userId: string; action: string; note: string; sourceVersion: number;
      requestKey: string; requestHash: string; now: number;
    }): Promise<void> {
      const stmt = await nativeSystemNotificationStatement(db, facts.companyId,
        {
          userId: facts.userId, recordType: facts.action, recordId: facts.bindingId,
          recordVersion: facts.sourceVersion, message: facts.note, kind: 'customer_portal_action',
          order_id: facts.orderId, identifier: facts.customerId,
          target_id: facts.requestKey, source_hash: facts.requestHash,
        }, facts.id, facts.now);
      await stmt.run();
    },
  };
}
