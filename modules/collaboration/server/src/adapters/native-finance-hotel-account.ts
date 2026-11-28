import 'server-only';
import type { SqlDatabase } from '@erpx/platform-runtime-server/public/runtime/types';
import { nativeSystemNotificationStatement } from './native-system-notification';

/** Native-owned hotel PMS account creation statement. */
export function financeHotelAccount(db: SqlDatabase) {
  return {
    async create(facts: {
      id: string; companyId: string; chartId: string; code: string; name: string;
      category: string; isControl: number; controlKind: string; now: number;
    }): Promise<void> {
      const stmt = await nativeSystemNotificationStatement(db, facts.companyId,
        {
          userId: '', recordType: facts.category, recordId: facts.code,
          recordVersion: facts.isControl, message: facts.name, kind: 'finance_hotel_account',
          identifier: facts.chartId, item_id: facts.controlKind,
        }, facts.id, facts.now);
      await stmt.run();
    },
  };
}
