/** G4 / retire.finance.read-tail-r30 byte-identical guard-prefix read families.
 *
 * Each prefix below is emitted byte-for-byte as the pre-dedupe inline SQL.
 * Call sites concatenate the family prefix with their own unchanged suffix;
 * row-lock suffixes stay inline in the calling adapters. The retained tax
 * assertion guard `pendingCorrectionsSql` is intentionally not touched.
 */
export const ACCOUNT_SELECT_PREFIX = "SELECT id FROM finance_accounts WHERE ";
export const INVOICE_EXISTS_PREFIX = "EXISTS(SELECT 1 FROM finance_invoices WHERE id=? AND company_id=?";
export const JOURNAL_SELECT_PREFIX = "SELECT 1 FROM finance_journal";
