/** Parameterized Billing read-tail r28 SQL text shared by the evidence storage
 * ledger point read and the stored-totals projections. Text only: every caller
 * supplies its own projection columns and predicate and executes its own
 * statement with its own binds; nothing is cached or executed here. */
export const billingStoredLedgerSql = (columns: string, where: string) =>
  `SELECT ${columns} FROM billing_evidence_storage_ledger WHERE ${where}`;
