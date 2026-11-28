/** Byte-identical finance_journal_lines projection text shared by the Finance owner-projection sources.
 * Text only: no query is prepared or executed here and no result is cached. */
export const FINANCE_JOURNAL_LINES_PROJECTION_SQL = "SELECT \"id\",\"journal_id\",\"account_id\",\"description\",\"debit_minor\",\"credit_minor\",\"base_debit_minor\",\"base_credit_minor\",\"dimension_json\",\"ordinal\" FROM finance_journal_lines";
