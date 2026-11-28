import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import { nativeLane28StaffLevels, type Lane28WorkContext } from './native-lane-28/client';

/** Native owner read: the count of active numbered staff levels (1..6) used
 *  by company acceptance. The retired inline count projection is owned and
 *  executed by the Rust lane-28 router; no SQL remains in this adapter.
 *  The caller's unit is retained for transport compatibility. */
export function companyAcceptanceFacts(_db: SqlDatabase, actor?: Lane28WorkContext) {
  return {
    async levels(companyId: string): Promise<{ count: number }> {
      const facts = await nativeLane28StaffLevels(companyId, actor);
      return { count: facts.activeCount };
    },
  };
}
