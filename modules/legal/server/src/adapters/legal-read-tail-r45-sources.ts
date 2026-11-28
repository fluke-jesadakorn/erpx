import "server-only";

/** One parameterized read head for the Legal item-by-id-and-company witness
 * projections. Only the trusted projection and the trusted key-column prefix
 * vary; the table, alias and both equality binds are fixed. No request value
 * reaches the head: every caller supplies module-local SQL constants. */
export const legalItemsByIdCompanyHead = (projection: string, keyPrefix = "") =>
  `SELECT ${projection} FROM legal_items i WHERE ${keyPrefix}id=? AND ${keyPrefix}company_id=?`;
