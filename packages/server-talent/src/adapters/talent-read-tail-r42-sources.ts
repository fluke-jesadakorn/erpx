import "server-only";
/** Owner-local source-row select head family (round 42). The two company-scoped
 *  product-entry read gates share one byte-identical `<select> projection FROM`
 *  head; each caller keeps its own projection, joins, predicate and binds, so
 *  the emitted SQL stays byte-identical to the former inline literals. Distinct
 *  from the r37 projection-only head (which stops before FROM) and from the
 *  EXISTS, lock and native-read families. Text only: nothing is cached or
 *  executed here. */
export const selectSourceRow = (projection: string) => `SELECT ${projection} FROM`;
