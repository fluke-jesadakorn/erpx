/** G4 / retire.finance.read-tail-r33 byte-identical read families.
 *
 * Each constant is rendered byte-for-byte as the pre-dedupe inline SQL.
 * No row-lock text is included; retained assertion guards stay inline at the
 * calling adapters. r15-r32 helpers are untouched.
 */
export const PERIOD_BY_ID_SQL = "SELECT * FROM finance_periods WHERE id=? AND company_id=?";
