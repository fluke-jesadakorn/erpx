/** Byte-identical finance_charts projection text shared by the Finance owner-projection sources.
 * Text only: no query is prepared or executed here and no result is cached. */
export const FINANCE_CHARTS_PROJECTION_SQL = "SELECT \"id\",\"company_id\",\"name\",\"base_currency\",\"accounting_basis\",\"status\",\"version\",\"created_by\",\"created_at\",\"updated_at\" FROM finance_charts";
