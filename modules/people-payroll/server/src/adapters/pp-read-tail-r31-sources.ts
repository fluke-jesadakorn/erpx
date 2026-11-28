import 'server-only';
/** Canonical parameterized SELECT-only owner read guard shared across People Payroll
 * adapters (G4 read-tail r31). One read-only existence literal owns the SQL; consumers import
 * the builder and keep no local copy. This is a pure read (no INSERT/UPDATE/DELETE,
 * LOCK TABLE, FOR UPDATE/SHARE or advisory lock) and is not covered by the r19-r30 canonical
 * modules, which own only the wider id/company/version/status guards. Lock-bearing reads and
 * cross-owner predicates stay untouched. */

/** Single-column status existence guard; `negated` selects the NOT EXISTS form. */
export const ppRd31StatusGuard = (negated: boolean, table: string, col: string, status: string) =>
  `${negated ? 'NOT ' : ''}EXISTS(SELECT 1 FROM ${table} WHERE ${col}=? AND status='${status}')`;
