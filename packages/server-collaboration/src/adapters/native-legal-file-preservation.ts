import 'server-only';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import { nativeSystemNotificationStatement } from './native-system-notification';

/** Native-owned legal attachment preservation statement. */
export function legalFilePreservation(db: SqlDatabase) {
  return {
    async preserve(facts: { id: string; companyId: string; sourceId: string; objectKey: string; checksum: string; userId: string; now: number }): Promise<void> {
      const stmt = await nativeSystemNotificationStatement(db, facts.companyId,
        {
          userId: facts.userId, recordType: '', recordId: facts.id, recordVersion: 0,
          message: facts.checksum, kind: 'legal_file_preservation',
          identifier: facts.sourceId, item_id: facts.objectKey,
        }, facts.id, facts.now);
      await stmt.run();
    },
  };
}
