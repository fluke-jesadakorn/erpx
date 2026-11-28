/** G4 / retire.finance.read-tail-r56 closed read-tail renderers.
 *
 * Every export renders byte-identical text to the literal it replaces: the
 * head, output columns, table, bind order and raw tail are preserved. Callers
 * keep their tail text, binds and raw-text parsing; nothing here is cached or
 * executed. Locks, native plan kinds, row-exclusive set reads, r54/r55 helper
 * bodies and every r15-r53 helper stay inline and untouched.
 */

/** Bank-statement match guard. `tail` is raw SQL after `t.id=?` without the
 * closing `)` of the EXISTS wrapper. */
export const bankMatchGuardSql = (tail: string) =>
  `EXISTS(SELECT 1 FROM finance_bank_transactions t JOIN finance_bank_statements s ON s.id=t.statement_id WHERE t.id=?${tail})`;

/** Company-first row list. `tail` is raw SQL text after `company_id=?`. */
export const companyIdRowsSql = (columns: string, table: string, tail: string) =>
  `SELECT ${columns} FROM ${table} WHERE company_id=?${tail}`;

/** Customer receipt point read by payment. `tail` is raw SQL after
 * `payment_id=?`. */
export const receiptIdByPaymentSql = (tail: string) =>
  `SELECT id FROM finance_customer_receipts WHERE payment_id=?${tail}`;
