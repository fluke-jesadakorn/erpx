import 'server-only';
/** Canonical parameterized SELECT-only owner read shared across People Payroll
 * adapters (G4 read-tail r32). One read-only two-column projection owns the SQL;
 * consumers import the builder and keep no local copy. The projection selects
 * exactly two columns and the WHERE filters by exactly two columns: `col=? AND
 * company_id=?`, or the reversed `company_id=? AND col=?` (bind company first).
 * This is a pure read (no INSERT/UPDATE/DELETE, LOCK TABLE, FOR UPDATE/SHARE or
 * advisory lock) and is not covered by the r1-r31 canonical modules. Lock-bearing
 * reads and cross-owner predicates stay untouched. */

/** Two-column projection by two predicates; c1/c2 fix the bind order. */
export const ppRd32TwoColProjection = (cols: string, table: string, c1: string, c2: string) =>
  `SELECT ${cols} FROM ${table} WHERE ${c1}=? AND ${c2}=?`;
