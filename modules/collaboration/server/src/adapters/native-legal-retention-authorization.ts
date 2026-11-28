import 'server-only';
import type { SqlDatabase } from '@erpx/platform-runtime-server/public/runtime/types';
import { nativeSystemNotificationStatement } from './native-system-notification';

/** Native-owned legal retention execution authorization statement. */
export function legalRetentionAuthorization(db: SqlDatabase) {
  return {
    async authorize(facts: {
      key: string; companyId: string; destructionId: string; itemId: string; kind: string;
      userId: string; assignmentId: string; authority: string; sourceHash: string;
      itemVersion: number; now: number;
    }): Promise<void> {
      const stmt = await nativeSystemNotificationStatement(db, facts.companyId,
        {
          userId: facts.userId, recordType: facts.kind, recordId: facts.destructionId,
          recordVersion: facts.itemVersion, message: facts.authority,
          kind: 'legal_retention_authorization', item_id: facts.itemId,
          identifier: facts.assignmentId, source_hash: facts.sourceHash,
        }, facts.key, facts.now);
      await stmt.run();
    },
  };
}
