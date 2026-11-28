/** Procurement read-tail r33 shared fragment. Text only: each builder renders the
 * exact original statement so binds, ordering, locks and writes are unchanged.
 * r17-r32 helpers, FOR UPDATE/FOR SHARE and command_receipts stay untouched. */
/** Closed quoted-identifier projection used by the final owner projection
 * sources. The quoted column list and table render verbatim; no bind, lock,
 * predicate or alias is introduced. */
export const quotedProjection = (columns: string, table: string) =>
  `SELECT ${columns} FROM ${table}`;
