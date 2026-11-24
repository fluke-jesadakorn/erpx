import "server-only";
/** Parameterized SELECT-only idempotency/receipt probe family shared by the
 * Commercial grant store (G4 read-tail r5). The builder owns one SQL literal and
 * renders each original read byte-identically; every consumer still prepares and
 * binds its own independent statement and keeps its exports. Arguments are
 * code-owned table/column fragments only, never request input. The r1/r3/r4
 * helpers stay untouched. Pure read: no INSERT, UPDATE, DELETE, LOCK TABLE,
 * FOR UPDATE/SHARE and no advisory lock. */
export function receiptProbe(table: string, columns: string) {
  return `SELECT ${columns} FROM ${table} WHERE id=?`;
}
