/** Byte-identical finance_journals projection text shared by the Finance owner-projection sources.
 * Text only: no query is prepared or executed here and no result is cached. */
export const FINANCE_JOURNALS_PROJECTION_SQL = "SELECT \"id\",\"company_id\",\"chart_id\",\"period_id\",\"journal_number\",\"entry_date\",\"currency\",\"fx_rate_micros\",\"source_type\",\"source_id\",\"memo\",\"status\",\"is_opening\",\"version\",\"created_by\",\"posted_by\",\"posted_at\",\"reversed_by\",\"reversed_at\",\"created_at\",\"updated_at\" FROM finance_journals";
