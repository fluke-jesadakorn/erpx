/** Byte-identical Finance read-tail SQL shared across point reads and owner projections.
 * Text only: every call site still prepares and executes with its own binds and order. */
export const OPEN_PERIOD_GUARD_SQL = "EXISTS(SELECT 1 FROM finance_periods WHERE id=? AND company_id=? AND state='open' AND start_date<=? AND end_date>=?)";
export const OPEN_PERIOD_IDS_SQL = "SELECT id FROM finance_periods WHERE company_id=? AND state='open' AND start_date<=? AND end_date>=?";
