import "server-only";
import { scopedReadHead } from "./commercial-read-tail-r14-sources";
/** Parameterized holding+month read family shared by the Commercial seat meter
 * repository (G4 read-tail r7). The single scoped head owns the SQL literal and
 * renders both original reads byte-identically; every consumer still prepares and
 * binds its own independent statement and keeps its exports. Arguments are
 * code-owned table/column fragments only, never request input.
 * Pure read: no writes, no table locks and no advisory locks. */
export function holdingMonthReadSql(columns: string, table: string) {
  return scopedReadHead(columns, table, 'holding-month');
}
