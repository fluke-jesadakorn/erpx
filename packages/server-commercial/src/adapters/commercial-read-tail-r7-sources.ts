import "server-only";
/** Parameterized holding+month read family shared by the Commercial seat meter
 * repository (G4 read-tail r7). The builder owns one SQL literal and renders both
 * original reads byte-identically (whitespace-normalised); every consumer still
 * prepares and binds its own independent statement and keeps its exports.
 * Arguments are code-owned table/column fragments only, never request input.
 * Pure read: no INSERT, UPDATE, DELETE, LOCK TABLE, FOR UPDATE/SHARE and no
 * advisory lock. The r1/r3/r4/r5/r6 helpers stay untouched. */
export function holdingMonthReadSql(columns: string, table: string) {
  return `SELECT ${columns} FROM ${table} WHERE holding_id=? AND month=?`;
}
