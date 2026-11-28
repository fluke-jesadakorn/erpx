/** Procurement read-tail r35 shared fragment. Text only: the builder renders the
 * exact original statement so binds, ordering, locks and writes are unchanged.
 * r17-r34 helpers, FOR UPDATE/FOR SHARE locks and command_receipts stay untouched. */
/** Quoted-identifier FROM projection used by the named notification snapshot
 * sources. The quoted column list, source expression and alias render verbatim;
 * no bind, lock or predicate is introduced. */
export const quotedFromSource = (columns: string, source: string, alias: string) =>
  `FROM (SELECT ${columns} FROM ${source}) ${alias}`;
