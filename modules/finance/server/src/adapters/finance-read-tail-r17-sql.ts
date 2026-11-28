/** Byte-identical Finance read-tail SQL shared across point reads and owner projections.
 * Text only: every call site still prepares and executes with its own binds and order. */
export const SUPPLIER_RESERVATION_GUARD_SQL = "NOT EXISTS(SELECT 1 FROM finance_supplier_instructions WHERE invoice_id=? AND status IN ('reserved','exported','unknown'))";
export const BANK_TRANSACTION_BY_ID_SQL = "SELECT t.*,s.bank_account FROM finance_bank_transactions t JOIN finance_bank_statements s ON s.id=t.statement_id WHERE t.id=? AND t.company_id=?";
export const DIMENSION_DEFINITIONS_SQL = "SELECT id,code,name,required FROM finance_dimension_definitions WHERE company_id=? AND status='active' ORDER BY code";
