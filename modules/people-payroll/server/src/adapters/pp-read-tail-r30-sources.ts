import 'server-only';
/** Canonical parameterized SELECT-only owner read guard shared across People Payroll
 * adapters (G4 read-tail r30). One read-only EXISTS literal owns the SQL; consumers import
 * the builder and keep no local copy. This is a pure read (no INSERT/UPDATE/DELETE,
 * LOCK TABLE, FOR UPDATE/SHARE or advisory lock) and is not covered by r19-r29 canonical
 * modules. Lock-bearing reads and cross-owner predicates stay untouched. */

/** Four-column existence guard keyed by a table and its four predicate columns. */
export const ppRd30Exists4Guard = (table: string, c1: string, c2: string, c3: string, c4: string) =>
  `EXISTS(SELECT 1 FROM ${table} WHERE ${c1}=? AND ${c2}=? AND ${c3}=? AND ${c4}=?)`;
