/** Procurement read-tail r41 shared fragment. Text only: the builder renders the
 * exact original parenthesized subquery so binds, ordering, locks and writes are
 * unchanged. r17-r40 helpers, FOR UPDATE/FOR SHARE locks and command_receipts
 * stay untouched. */
/** Open-promise-line aggregate witness shared by the supplier-promise workRows
 * followup-date and promised subqueries. The caller's projection and trailing
 * predicate render verbatim; no bind, lock or predicate is introduced. */
export const openPromiseByOrder = (projection: string, tail = "") =>
  `(SELECT ${projection} FROM procurement_open_promise_lines p JOIN procurement_delivery_promises h ON h.id=p.promise_id WHERE h.order_id=o.id${tail})`;
