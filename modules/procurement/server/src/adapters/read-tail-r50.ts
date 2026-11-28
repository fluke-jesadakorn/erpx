/** G4 / retire.procurement.read-tail-r50 safe NEW read-tail SQL family.
 *
 * Every export renders byte-identical text to the literal it replaces. Callers
 * keep their own binds, bind order, result types and transaction; nothing here
 * is cached or executed. `alias` is raw SQL text (a quoted alias or an
 * interpolated one). Row locks, advisory locks, native writes and prior
 * r17-r49 helpers stay untouched.
 */

/** Bill-match receipt existence head shared by the requisition, purchase-order
 * and supplier tails. The tail (JOIN/WHERE and every bind) stays at the caller. */
export const billMatchReceiptHead = (alias: string) =>
  `SELECT 1 FROM procurement_bill_match_receipts ${alias}`;
