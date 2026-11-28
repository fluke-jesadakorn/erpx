/** Procurement read-tail r39 shared fragment. Text only: the builder renders the
 * exact original parenthesized source-exists witness so binds, ordering, locks
 * and writes are unchanged. r17-r38 helpers, FOR UPDATE/FOR SHARE locks and
 * command_receipts stay untouched. */
export const actualSourceExists = (table: string, alias: string, id: string) =>
  `(SELECT 1 FROM ${table} ${alias} WHERE ${alias}.id=${id})`;
