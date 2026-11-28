/** G4 / retire.finance.read-tail-r42 safe NEW read-tail SQL families.
 *
 * Every export renders byte-identical text to the literal it replaces. Callers
 * keep their own binds, result types and transaction; nothing here is cached or
 * executed. Row-lock, tax, assertion, native and r15-r41 helpers remain untouched.
 */

export const financeAiProposalPointSql = (tail: string) => `SELECT * FROM finance_ai_proposals WHERE id=? AND company_id=?${tail}`;

export const payrollPostPeriodSql = (tail: string) => `SELECT id,start_date FROM finance_periods WHERE company_id=? AND state='open'${tail}`;

export const financeReferenceListSql = (table: string, predicate: string, orderBy: string) => `SELECT * FROM ${table} WHERE ${predicate} ORDER BY ${orderBy}`;

export const accrualScheduleRowsSql = (tail: string) => `SELECT s.*,da.code debit_code,da.name debit_name,ca.code credit_code,ca.name credit_name FROM finance_accrual_schedules s JOIN finance_accounts da ON da.id=s.debit_account_id JOIN finance_accounts ca ON ca.id=s.credit_account_id ${tail}`;

export const accrualRunRowsSql = (tail: string) => `SELECT r.*,s.description FROM finance_accrual_schedule_runs r JOIN finance_accrual_schedules s ON s.id=r.schedule_id ${tail}`;

export const consolidationGroupAccountsSql = (tail: string) => `SELECT a.id,a.company_id,a.chart_id,a.code,a.name,a.currency,c.base_currency\n${tail}`;

export const eliminationPendingSql = (statusPredicate: string) => `SELECT 1 FROM finance_consolidation_eliminations WHERE run_id=? AND ${statusPredicate} LIMIT 1`;

export const billingCorrectionPointSql = (tail: string) => `SELECT * FROM finance_billing_corrections WHERE id=? AND company_id=?${tail}`;

export const intercompanyPairPointSql = (tail: string) => `SELECT * FROM finance_intercompany_document_pairs WHERE id=? AND holding_id=?${tail}`;

export const chartPointSql = (tail: string) => `SELECT id,base_currency,status FROM finance_charts WHERE id=? AND company_id=?${tail}`;

export const bankImportMarkerExistsSql = (markerPath: string) => `SELECT 1 FROM command_receipts WHERE operation='finance.bank.import' AND authority_json::jsonb->>'companyId'=? AND ${markerPath}=?`;
