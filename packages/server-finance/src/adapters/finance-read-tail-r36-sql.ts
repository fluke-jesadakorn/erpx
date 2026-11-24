/** G4 / retire.finance.read-tail-r36 exact duplicate two-site SQL families.
 *
 * Each constant is the byte-identical SQL text shared by exactly two sites.
 * Callers keep their own binds and result types; no row-lock variant,
 * tax assertion guard or r15-r35 helper is changed here. Text only: nothing is
 * cached or executed by this module.
 */
export const CHART_ID_BY_COMPANY_SQL = 'SELECT id FROM finance_charts WHERE id=? AND company_id=?';
export const CHART_EXISTS_BY_COMPANY_SQL = 'SELECT 1 FROM finance_charts WHERE id=? AND company_id=?';
export const CHART_IMAGE_BY_COMPANY_SQL = 'SELECT * FROM finance_charts WHERE id=? AND company_id=?';
export const PAYROLL_LINE_IMAGE_SQL = 'SELECT to_jsonb(l)::text image FROM finance_journal_lines l WHERE l.journal_id=? ORDER BY l.ordinal,l.id';
