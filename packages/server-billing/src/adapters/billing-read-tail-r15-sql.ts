/** Byte-identical Billing read-tail SQL shared across owner-local point reads.
 * Text only: every call site still executes with its own binds and result type. */
export const BILLING_INVOICE_BY_ID_SQL = "SELECT * FROM billing_invoices WHERE id=$1";
