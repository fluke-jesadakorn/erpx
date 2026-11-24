import "server-only";
/** Parameterized Holding-scoped usage-snapshot read family shared by the
 * Commercial usage projection (G4 read-tail r9). The builder owns one SQL
 * literal and renders each original read byte-identically (whitespace-
 * normalised); every consumer still prepares and binds its own independent
 * statement and keeps its exports. Both arguments are code-owned column and
 * trailing-fragment pieces only, never request input. The r1/r3/r4/r5/r6/r7/r8
 * helpers stay untouched. Pure read: no writes, no LOCK TABLE and no advisory
 * lock; any row-lock suffix remains in the caller's own trailing fragment. */
export function holdingSnapshotReadSql(columns: string, rest: string) {
  return `SELECT ${columns} FROM billing_usage_snapshots WHERE holding_id=?${rest}`;
}
