/** Procurement read-tail r48 shared fragment. Text only: one owner-local head
 * renderer for the two supplier-bill detail reads and the supplier-promise
 * detail read. Projections, JOIN/WHERE/ORDER text, binds and locks stay with
 * the callers; r17-r47 helpers stay untouched. */
export const supplierDetailSelect = (head: string, from: string, gap = " ") =>
  `SELECT ${head}${gap}FROM ${from}`;
