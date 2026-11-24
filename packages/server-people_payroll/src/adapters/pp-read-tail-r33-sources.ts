import 'server-only';
/** Canonical parameterized SELECT-only owner reads shared across People Payroll
 * adapters (G4 read-tail r33). Each builder owns one literal copied verbatim from its
 * former inline copies; consumers import it and keep no local copy. These are pure
 * reads (no INSERT/UPDATE/DELETE, LOCK TABLE, FOR UPDATE/SHARE or advisory lock).
 * Lock-bearing reads, cross-owner predicates and r1-r32 modules stay untouched. */

/** Four-column projection by id+company; the bind order is always (id, company_id). */
export const ppRd33FourColByIdCompany = (cols: string, table: string) =>
  `SELECT ${cols} FROM ${table} WHERE id=? AND company_id=?`;

/** Select-star by two predicates; c1/c2 fix the bind order. */
export const ppRd33StarByTwoPredicates = (table: string, c1: string, c2: string) =>
  `SELECT * FROM ${table} WHERE ${c1}=? AND ${c2}=?`;
