import 'server-only';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import { nativeSystemNotificationStatement } from './native-system-notification';

/** Native-owned customer receipt statement. */
export function customerReceipt(db: SqlDatabase) {
  return {
    async issue(facts: {
      id: string; companyId: string; paymentId: string; number: string;
      paymentDate: string; currency: string; amountMinor: string;
      snapshot: string; html: string; hash: string; userId: string; now: number;
    }): Promise<void> {
      const stmt = await nativeSystemNotificationStatement(db, facts.companyId,
        {
          userId: facts.userId, recordType: '', recordId: facts.paymentId, recordVersion: 0,
          message: facts.snapshot, kind: 'customer_receipt',
          identifier: facts.number, item_id: facts.paymentDate,
          target_kind: facts.currency, target_id: facts.amountMinor,
          event: facts.html, source_hash: facts.hash,
        }, facts.id, facts.now);
      await stmt.run();
    },
  };
}
