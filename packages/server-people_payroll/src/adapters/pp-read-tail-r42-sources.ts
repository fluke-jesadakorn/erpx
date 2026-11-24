import 'server-only';
/** Canonical parameterized SELECT-only owner read builders shared across People Payroll
 * adapters (G4 read-tail r42). Each builder owns one read-only literal copied verbatim from
 * its former inline copies; consumers import it and keep no local copy. Every family here
 * is a pure read (no INSERT/UPDATE/DELETE, LOCK TABLE, FOR UPDATE/SHARE or advisory lock)
 * and is not covered by the r1-r41 canonical modules. Lock-bearing reads, native kinds,
 * cross-owner projections and r1-r41 modules stay untouched. */

/** Active, unlinked person point by two predicates; bind order is (c1, c2). */
export const ppRd42ActivePersonPoint = (cols: string, table: string, c1: string, c2: string) =>
  `SELECT ${cols} FROM ${table} WHERE ${c1}=? AND ${c2}=? AND status='active' AND merged_into_id IS NULL`;

/** Four-column owner projection head with no filter; no binds. */
export const ppRd42FourColOwnerProjection = (cols: string, table: string) =>
  `SELECT ${cols} FROM ${table}`;

/** id/version plus one literal-column EXISTS guard; bind order is (id, version, col). */
export const ppRd42IdVersionLiteralGuard = (table: string, col: string) =>
  `EXISTS(SELECT 1 FROM ${table} WHERE id=? AND version=? AND ${col}=?)`;
