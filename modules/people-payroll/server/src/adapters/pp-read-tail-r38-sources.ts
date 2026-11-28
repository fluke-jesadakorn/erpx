import 'server-only';
/** Canonical parameterized SELECT-only owner read builder shared across People Payroll
 * adapters (G4 read-tail r38). The builder owns one read-only literal copied verbatim from
 * its former inline copies; consumers import it and keep no local copy. It is a pure read
 * (no INSERT/UPDATE/DELETE, LOCK TABLE, FOR UPDATE/SHARE or advisory lock) and is not
 * covered by the r1-r37 canonical modules. Lock-bearing reads, cross-owner projections,
 * native kinds and r1-r37 modules stay untouched. */

/** Single-predicate projection with an optional ORDER BY tail. Bind order: (col). */
export const ppRd38SinglePredTail = (cols: string, table: string, col: string, tail = '') =>
  `SELECT ${cols} FROM ${table} WHERE ${col}=?${tail}`;
