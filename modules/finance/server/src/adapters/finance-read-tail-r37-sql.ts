/** G4 / retire.finance.read-tail-r37 exact duplicate two-site SQL family.
 *
 * The constant is the byte-identical SQL payload shared by two adapter sites
 * (one double-quoted, one single-quoted inline literal). Callers keep their own
 * binds and result types; no row-lock text, tax assertion guard or r15-r36
 * helper is changed here. Text only: nothing is cached or executed by this
 * module.
 */
export const PERIOD_EXISTS_BY_ID_SQL = 'SELECT 1 FROM finance_periods WHERE id=? AND company_id=?';
