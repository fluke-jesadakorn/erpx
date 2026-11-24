/** G4 / retire.finance.read-tail-r45 safe NEW read-tail SQL families.
 *
 * Every export renders byte-identical text to the literal it replaces. Callers
 * keep their own binds, result types and transaction; nothing here is cached or
 * executed. Lock-taking, tax, advisory, native and r15-r44 helpers are untouched.
 */

/** Full existence guard in version-before-status order (the r20 builder reverses it). */
export const versionStatusGuard = (table: string) =>
  `EXISTS(SELECT 1 FROM ${table} WHERE id=? AND company_id=? AND version=? AND status='active')`;

/** An open period whose end date equals the supplied entry date. */
export const PERIOD_OPEN_END_DATE_GUARD_SQL = "EXISTS(SELECT 1 FROM finance_periods WHERE id=? AND company_id=? AND state='open' AND end_date=?)";

/** No other non-closed period overlaps the supplied start/end range. */
export const EARLIER_PERIODS_GUARD_SQL = "NOT EXISTS(SELECT 1 FROM finance_periods WHERE company_id=? AND id<>? AND start_date<=? AND end_date>=? AND state<>'closed')";

/** Active statutory chart on the THB ledger. The 'THB' literal stays in the shared text. */
export const ACTIVE_THB_CHART_GUARD_SQL = "EXISTS(SELECT 1 FROM finance_charts WHERE id=? AND company_id=? AND status='active' AND base_currency='THB')";

/** No approved statutory accounting profile for the company. */
export const NO_APPROVED_PROFILE_GUARD_SQL = "NOT EXISTS(SELECT 1 FROM finance_accounting_profiles WHERE company_id=? AND status='approved')";
