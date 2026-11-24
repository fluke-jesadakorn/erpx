import 'server-only';
/** Canonical parameterized SELECT-only owner read shared across People Payroll
 * adapters (G4 read-tail r34). The builder owns one projection literal copied verbatim
 * from its former inline copies; consumers import it and keep no local copy. It is a pure
 * read (no INSERT/UPDATE/DELETE, LOCK TABLE, FOR UPDATE/SHARE or advisory lock).
 * Lock-bearing reads, EXISTS guards, cross-owner predicates and r1-r33 modules stay untouched. */

/** Active pay group projection by id+company; the bind order is always (id, company_id). */
export const ppRd34ActivePayGroupProjection = (cols: string) =>
  `SELECT ${cols} FROM hr_pay_groups WHERE id=? AND company_id=? AND status='active'`;
