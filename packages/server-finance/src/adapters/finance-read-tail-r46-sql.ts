/** G4 / retire.finance.read-tail-r46 safe NEW read-tail SQL families.
 *
 * Every export renders byte-identical text to the literal it replaces. Callers
 * keep their own binds, result types and transaction; nothing here is cached or
 * executed. Row-lock, tax, advisory, native and r15-r45 helpers are untouched.
 */
export const accountChartGuardSql = (tail: string) =>
  `EXISTS(SELECT 1 FROM finance_accounts WHERE id=? AND company_id=? AND chart_id=?${tail})`;

export const partyKindGuardSql = (tail: string) =>
  `EXISTS(SELECT 1 FROM finance_parties WHERE id=? AND company_id=? AND kind=?${tail})`;

export const FX_RUN_POSTED_GUARD_SQL = "NOT EXISTS(SELECT 1 FROM finance_fx_revaluation_runs WHERE period_id=? AND chart_id=? AND status='posted')";
