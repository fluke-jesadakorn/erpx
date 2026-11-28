/** G4 / retire.finance.read-tail-r39 exact and parameterized SQL families.
 *
 * Each export holds only SQL text shared by adapter sites. Callers keep their
 * own binds, result types and guards; nothing here is cached or executed.
 * Row-lock text, tax assertion guards and r15-r38 helpers are not changed.
 */
export const bankCashLinesSql = (cashSource: string) => `SELECT l.id,l.account_id,l.description,l.debit_minor,l.credit_minor,j.id journal_id,j.journal_number,j.currency,j.entry_date,j.entry_date page_key
    FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id
    JOIN finance_accounts a ON a.id=l.account_id AND a.company_id=j.company_id
    WHERE j.company_id=? AND j.status='posted' AND a.control_kind='bank' AND j.is_opening=0
    AND (?='' OR j.chart_id=?) AND ${cashSource}`;

export const ASSET_DEPRECIATIONS_SQL = 'SELECT * FROM finance_asset_depreciations WHERE asset_id=? ORDER BY ordinal';
export const ASSET_DRAFT_DISPOSAL_SQL = "SELECT 1 FROM finance_asset_disposals WHERE asset_id=? AND status='draft'";
export const INTERCOMPANY_BY_ID_SQL = 'SELECT * FROM finance_intercompany_transactions WHERE id=? AND holding_id=?';

export const openingBalanceSql = (gap: string) => `SELECT l.account_id AS "accountId",l.dimension_json AS dimensions,SUM(l.base_debit_minor-l.base_credit_minor) AS minor${gap}FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id${gap}WHERE j.company_id=? AND j.chart_id=? AND j.status IN ('posted','reversed') AND j.entry_date<?${gap}GROUP BY l.account_id,l.dimension_json HAVING SUM(l.base_debit_minor-l.base_credit_minor)<>0 ORDER BY l.account_id,l.dimension_json`;

export const documentNumberSql = (kind: string) => `SELECT erpx_sales_document_number(?,'${kind}',?) number`;
export const moneySnapshotExistsSql = (sourceType: string) => `SELECT 1 FROM money_transaction_snapshots WHERE company_id=? AND source_type='${sourceType}' AND source_id=? AND amount_kind=?`;
export const posDailyAccountNetSql = (account: string, settings: string) => `SELECT COALESCE(SUM(l.debit_minor-l.credit_minor),0) n FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id
  ${settings} WHERE j.company_id=? AND j.entry_date=? AND j.status='posted' AND j.source_type='pos'
  AND l.dimension_json::jsonb->>'branchId'=? AND l.account_id=c.accounts_json::jsonb->>'${account}'`;
