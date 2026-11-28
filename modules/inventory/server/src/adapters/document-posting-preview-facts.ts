import 'server-only';
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import type { CountRow } from "@erpx/inventory-server/domain/counts";
import {
  nativeInventoryReadAll,
  nativeInventoryReadFirst,
  INVENTORY_RD_STOCK_COUNT_ROW,
  INVENTORY_RD_STOCK_COUNT_DELTAS,
} from './native-inventory-read';

export function documentPostingPreviewFacts(db: SqlDatabase) {
  return {
    /** Owner-native read: planned by `RD_STOCK_COUNT_ROW` and executed on the
     * caller's unit; this module carries no SQL for it. */
    stockCount: (companyId: string, id: string) =>
      nativeInventoryReadFirst<CountRow>(INVENTORY_RD_STOCK_COUNT_ROW, [companyId, id], db),
    /** Owner-native read: planned by `RD_STOCK_COUNT_DELTAS` and executed on the
     * caller's unit; this module carries no SQL for it. */
    stockDeltas: async (id: string) =>
      (await nativeInventoryReadAll<{ delta: number }>(INVENTORY_RD_STOCK_COUNT_DELTAS, [id], db)).results,
  };
}
