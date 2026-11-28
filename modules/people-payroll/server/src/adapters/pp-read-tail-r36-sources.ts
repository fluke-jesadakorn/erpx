import 'server-only';
/** Canonical parameterized SELECT-only owner read builders shared across People Payroll
 * adapters (G4 read-tail r36). Each builder owns one read-only literal copied verbatim from
 * its former inline copies; consumers import it and keep no local copy. Every family here
 * is a pure read (no INSERT/UPDATE/DELETE, LOCK TABLE, FOR UPDATE/SHARE or advisory lock)
 * and is not covered by the r1-r35 canonical modules. Lock-bearing reads, cross-owner
 * predicates and r1-r35 modules stay untouched. */

/** Family 1: single-predicate point projection. Bind order: (col). */
export const ppRd36SinglePred = (cols: string, table: string, col: string) =>
  `SELECT ${cols} FROM ${table} WHERE ${col}=?`;

/** Family 2: numbered $1/$2 two-predicate projection. Bind order: ($1, $2). */
export const ppRd36NumberedTwoPred = (cols: string, table: string, c1: string, c2: string) =>
  `SELECT ${cols} FROM ${table} WHERE ${c1}=$1 AND ${c2}=$2`;

/** Family 3: three-predicate projection. Bind order: (c1, c2, c3). */
export const ppRd36ThreePred = (cols: string, table: string, c1: string, c2: string, c3: string) =>
  `SELECT ${cols} FROM ${table} WHERE ${c1}=? AND ${c2}=? AND ${c3}=?`;

/** Family 4: two-predicate projection with optional tail. Bind order: (c1, c2). */
export const ppRd36TwoPredTail = (cols: string, from: string, c1: string, c2: string, tail = '') =>
  `SELECT ${cols} FROM ${from} WHERE ${c1}=? AND ${c2}=?${tail}`;
