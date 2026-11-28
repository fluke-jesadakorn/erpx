/** G4 / retire.finance.read-tail-r44 safe NEW read-tail SQL families.
 *
 * Every export renders byte-identical text to the literal it replaces. Callers
 * keep their own binds, result types and transaction; nothing here is cached or
 * executed. Row-lock, tax, assertion, native and r15-r43 helpers are untouched.
 */
export const FINANCE_SCOPE_BIND_SQL = 'SELECT ?::text company_id,?::text branch_id,?::text department_id';

export const FINANCE_SCOPE_NULL_BIND_SQL = 'SELECT ?::text company_id,NULL::text branch_id,NULL::text department_id';

export const jsonbArrayTextSql = (tail: string) => `SELECT jsonb_array_elements_text(?::jsonb)${tail}`;

export const UNMATCHED_BANK_TXN_COUNT_SQL = "SELECT count(*) FROM finance_bank_transactions WHERE company_id=? AND match_status<>'matched'";

export const AI_USAGE_MONTH_SQL = 'SELECT company_id,month FROM finance_ai_usage';
