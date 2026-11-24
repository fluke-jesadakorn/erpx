/** G4 / retire.finance.read-tail-r34 byte-identical read families.
 *
 * Each constant is rendered byte-for-byte as the pre-dedupe inline SQL.
 * No row-lock text is included; retained assertion guards and lock variants
 * (pms-command-facts FOR SHARE, pos-core-facts FOR UPDATE) stay inline at the
 * calling adapters. r15-r33 helpers are untouched.
 */
export const OPEN_PERIOD_COVERED_SQL = "SELECT 1 FROM finance_periods WHERE id=? AND company_id=? AND state='open' AND start_date<=? AND end_date>=?";
export const OPEN_PERIOD_LIST_ASC_SQL = "SELECT id,name,start_date,end_date FROM finance_periods WHERE company_id=? AND state='open' ORDER BY start_date";
export const BANK_TRANSACTION_STATEMENT_SQL = "SELECT t.*,s.bank_account,s.statement_number FROM finance_bank_transactions t JOIN finance_bank_statements s ON s.id=t.statement_id AND s.company_id=t.company_id WHERE t.id=? AND t.company_id=?";
export const ACTIVE_CHART_NAV_SQL = "SELECT id,name,base_currency FROM finance_charts WHERE company_id=? AND status='active' ORDER BY name";
export const ACTIVE_ACCOUNT_CHART_ID_SQL = "SELECT chart_id FROM finance_accounts WHERE id=? AND company_id=? AND status='active'";
