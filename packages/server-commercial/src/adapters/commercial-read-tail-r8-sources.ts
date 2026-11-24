import "server-only";
import { scopedReadHead } from "./commercial-read-tail-r14-sources";
/** Parameterized Company-scoped star-read family shared by the Commercial
 * grant store (G4 read-tail r8). The single scoped head owns the SQL literal and
 * renders each original read byte-identically; every consumer still prepares and
 * binds its own independent statement and keeps its exports. Both arguments are
 * code-owned table/trailing-fragment pieces only, never request input. Pure read:
 * no writes, no table locks and no advisory locks; any row-lock suffix remains in
 * the caller's own trailing fragment. */
export function companyScopedStarSql(table: string, rest: string) {
  return scopedReadHead('*', table, 'company', ' ', rest);
}
