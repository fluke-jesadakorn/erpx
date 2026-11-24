/** Procurement read-tail r38 shared fragment. Read-only text only: the builder
 * renders the exact original parenthesized witness so binds, ordering, locks
 * and writes are unchanged. r17-r37 helpers, FOR UPDATE/FOR SHARE locks and
 * command_receipts stay untouched. */
export const lineSourceWitness = (source: string, fk: string, ref: string) =>
  `(SELECT 1 FROM (${source}) line WHERE line.${fk}=${ref})`;
