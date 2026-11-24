/** G4 / retire.finance.read-tail-r32 byte-identical read families.
 *
 * Each constant is rendered byte-for-byte as the pre-dedupe inline SQL.
 * No row-lock text is included; retained assertion guards stay inline at the
 * calling adapters. r15-r31 helpers are untouched.
 */
export const ACTIVE_CHART_BASE_CURRENCY_BY_ID_SQL = "SELECT id,base_currency FROM finance_charts WHERE id=? AND company_id=? AND status='active'";
