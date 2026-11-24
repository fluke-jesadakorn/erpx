/** G4 / retire.finance.read-tail-r31 byte-identical read families.
 *
 * Each constant is rendered byte-for-byte as the pre-dedupe inline SQL.
 * No row-lock text is included; retained assertion guards stay inline at the
 * calling adapters. r15-r30 helpers are untouched.
 */
export const CONSOLIDATION_RUN_BY_ID_SQL = "SELECT * FROM finance_consolidation_runs r WHERE id=? AND holding_id=?";
export const ISSUED_THAI_DOCUMENT_BY_ID_SQL = "SELECT * FROM finance_issued_thai_documents WHERE id=? AND company_id=?";
