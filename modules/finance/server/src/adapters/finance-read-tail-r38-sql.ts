/** G4 / retire.finance.read-tail-r38 exact duplicate SQL families.
 *
 * Each constant is the byte-identical SQL text shared by adapter sites.
 * Callers keep their own binds and result types; no row-lock text, tax
 * assertion guard or r15-r37 helper is changed here. Text only: nothing is
 * cached or executed by this module.
 */
export const BILLING_RUN_CHILD_EXISTS_SQL = 'SELECT 1 FROM finance_business_billing_runs WHERE parent_run_id=?';
export const CUSTOMER_PARTY_PROFILE_SQL = "SELECT * FROM finance_parties WHERE id=? AND company_id=? AND kind='customer' AND person_id IS NULL";
export const BILLING_RUN_POINT_SQL = 'SELECT * FROM finance_business_billing_runs WHERE id=? AND company_id=?';
export const DIMENSION_VALUES_ACTIVE_SQL = "SELECT id,dimension_id,code,name FROM finance_dimension_values WHERE company_id=? AND status='active' ORDER BY code";
export const REFUND_INSTRUCTION_POINT_SQL = 'SELECT * FROM finance_refund_instructions WHERE id=? AND company_id=?';
export const BUSINESS_CONTRACT_POINT_SQL = 'SELECT * FROM finance_business_contracts WHERE id=? AND company_id=?';
export const ACCRUAL_SCHEDULE_POINT_SQL = 'SELECT * FROM finance_accrual_schedules WHERE id=? AND company_id=?';
export const CALCULATION_RUN_POINT_SQL = 'SELECT * FROM finance_calculation_runs WHERE id=? AND company_id=?';
export const PLAN_EDITION_POINT_SQL = 'SELECT * FROM finance_plan_editions WHERE id=? AND company_id=?';
export const BILLING_RUN_ROOT_WINDOW_SQL = 'SELECT 1 FROM finance_business_billing_runs WHERE root_id=? AND service_to>=?';
export const POS_CUSTOMER_PARTY_ACTIVE_SQL = "SELECT id FROM finance_parties WHERE id=? AND company_id=? AND kind='customer' AND status='active'";
export const POS_JOURNAL_LINE_NET_SQL = 'SELECT COALESCE(SUM(debit_minor-credit_minor),0) net FROM finance_journal_lines WHERE journal_id=? AND account_id=?';
export const SUPPLIER_RECEIPT_SOURCE_SQL = 'SELECT to_jsonb(r)::text source FROM command_receipts r WHERE r.command_id=?';
