/** Byte-identical Finance read-tail SQL shared across point reads and owner projections.
 * Text only: every call site still prepares and executes with its own binds and order. */
export const CHART_BASE_CURRENCY_BY_ID_SQL = "SELECT base_currency FROM finance_charts WHERE id=? AND company_id=?";
export const OPEN_PERIOD_LIST_SQL = "SELECT id,name,start_date,end_date FROM finance_periods WHERE company_id=? AND state='open' ORDER BY start_date DESC";
export const APPROVED_SECONDARY_CHART_RULE_SQL = "SELECT 1 FROM finance_book_rules WHERE company_id=? AND secondary_chart_id=? AND status='approved'";
export const FINANCE_JOURNALS_JOIN_SQL = "JOIN (SELECT \"id\",\"company_id\",\"chart_id\",\"period_id\",\"journal_number\",\"entry_date\",\"currency\",\"fx_rate_micros\",\"source_type\",\"source_id\",\"memo\",\"status\",\"is_opening\",\"version\",\"created_by\",\"posted_by\",\"posted_at\",\"reversed_by\",\"reversed_at\",\"created_at\",\"updated_at\" FROM finance_journals) j";
export const COMMAND_RECEIPT_EXISTS_SQL = "SELECT 1 FROM command_receipts WHERE command_id=? AND operation=? AND actor_id=? AND authority_json::jsonb->>'companyId'=?";
export const COMMAND_RECEIPT_RECORD_IMAGE_SQL = "SELECT r.record_id,to_jsonb(r)::text image FROM command_receipts r WHERE r.command_id=? AND r.operation=? AND r.actor_id=? AND r.authority_json::jsonb->>'companyId'=?";
