import "server-only";
/** Parameterized Company-scoped star-read family shared by the Commercial
 * grant store (G4 read-tail r8). The builder owns one SQL literal and renders
 * each original read byte-identically; every consumer still prepares and binds
 * its own independent statement and keeps its exports. Both arguments are
 * code-owned table/trailing-fragment pieces only, never request input. The
 * r1/r3/r4/r5/r6/r7 helpers stay untouched. Pure read: no writes, no LOCK
 * TABLE and no advisory lock; any row-lock suffix remains in the caller's
 * own trailing fragment. */
export function companyScopedStarSql(table: string, rest: string) {
  return `SELECT * FROM ${table} WHERE company_id=?${rest}`;
}
