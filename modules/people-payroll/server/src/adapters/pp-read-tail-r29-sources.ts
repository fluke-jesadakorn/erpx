import 'server-only';
/** Canonical parameterized SELECT-only owner read fragments shared across People Payroll
 * adapters (G4 read-tail r29). Each builder/constant owns one read-only literal copied verbatim
 * from its former inline copies; consumers import it and keep no local copy. Every family here
 * is a pure read (no INSERT, UPDATE, DELETE, LOCK TABLE, FOR UPDATE/SHARE or advisory lock)
 * and is not covered by the r19-r28 canonical modules. Lock-bearing reads stay untouched. */

/** Open finance-period guard used by expense create/update authorization. */
export const ppRd29OpenFinancePeriodSql = (periods: string) =>
  `EXISTS(SELECT 1 FROM (${periods}) finance_periods WHERE id=? AND company_id=? AND state='open' AND start_date<=? AND end_date>=?)`;

/** Final-pay terminal absence guard for cancelled preparations. */
export const PP_RD29_FINAL_PAY_TERMINAL_SQL = `NOT EXISTS(SELECT 1 FROM final_pay_terminals fp WHERE fp.preparation_id=p.id AND fp.company_id=p.company_id AND fp.version=p.version)`;

/** Active IAM role witness keyed by an assignment role and company. */
export const PP_RD29_IAM_ROLE_ACTIVE_SQL = `EXISTS(SELECT 1 FROM iam_roles role WHERE role.id=a.role AND role.company_id IN ('*',a.company_id) AND role.status='active')`;

/** Worker-placement terminal absence guard. `rows` is the owner placement terminal rows. */
export const ppRd29PlacementTerminalSql = (rows: string) =>
  `NOT EXISTS(SELECT 1 FROM (${rows}) terminal WHERE terminal.id=c.id AND terminal.company_id=c.company_id)`;

/** Open lifecycle case for an engagement (ended-employee retention). */
export const PP_RD29_LIFECYCLE_CASE_FOR_ENGAGEMENT_SQL = `EXISTS(SELECT 1 FROM people_lifecycle_cases lc WHERE lc.engagement_id=e.id AND lc.company_id=e.company_id)`;

/** Finance-expense treasury-scoped guard. `treasury` is the expense treasury predicate. */
export const ppRd29FinanceExpenseTreasurySql = (treasury: string) =>
  `EXISTS(SELECT 1 FROM finance_expenses e WHERE e.id=? AND e.company_id=? AND (${treasury}))`;

/** Active supplier payee-party guard. `parties` is the finance-parties projection. */
export const ppRd29FinancePartySupplierSql = (parties: string) =>
  `EXISTS(SELECT 1 FROM (${parties}) finance_parties WHERE id=? AND company_id=? AND kind='supplier' AND status='active' AND currency=?)`;
