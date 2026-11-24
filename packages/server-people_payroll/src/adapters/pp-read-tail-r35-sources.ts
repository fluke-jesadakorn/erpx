import 'server-only';
/** Canonical parameterized SELECT-only owner read shared across People Payroll
 * adapters (G4 read-tail r35). The builder owns one projection literal copied verbatim
 * from its former inline copies; consumers import it and keep no local copy. It is a pure
 * read (no INSERT/UPDATE/DELETE, LOCK TABLE, FOR UPDATE/SHARE or advisory lock).
 * Lock-bearing reads, EXISTS guards and r1-r34 modules stay untouched. */

/** Payroll item projection by run id; tail carries the optional ORDER BY suffix.
 * The bind order is always (run_id). */
export const ppRd35PayrollItemsByRun = (cols: string, tail = '') =>
  `SELECT ${cols} FROM hr_payroll_items WHERE run_id=?${tail}`;
