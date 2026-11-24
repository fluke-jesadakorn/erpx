/** Byte-identical active-chart base-currency read text shared by Finance adapters.
 * Text only: every call site still prepares and executes its own statement.
 * The native share-locking variant is intentionally not adopted here. */
export const ACTIVE_CHART_BASE_CURRENCY_SQL = "SELECT base_currency FROM finance_charts WHERE id=? AND company_id=? AND status='active'";
