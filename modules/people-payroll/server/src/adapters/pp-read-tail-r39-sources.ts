import 'server-only';
/** Canonical parameterized SELECT-only owner read builder shared across People Payroll
 * adapters (G4 read-tail r39). The builder owns one read-only literal copied verbatim from
 * its former inline copies; consumers import it and keep no local copy. It is a pure read
 * (no INSERT/UPDATE/DELETE, LOCK TABLE, FOR UPDATE/SHARE or advisory lock) and is not
 * covered by the r1-r38 canonical modules. Lock-bearing reads, cross-owner projections,
 * native kinds and prior builders stay untouched. */

/** Numbered three-predicate projection. Bind order: ($1, $2, $3). */
export const ppRd39NumberedThreePred = (cols: string, table: string, c1: string, c2: string, c3: string) =>
  `SELECT ${cols} FROM ${table} WHERE ${c1}=$1 AND ${c2}=$2 AND ${c3}=$3`;
