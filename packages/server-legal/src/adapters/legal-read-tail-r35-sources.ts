import "server-only";
/** Byte-identical Legal item-scoped ordered-list SQL shared by owner adapters (round 35).
 * Text only: every call site still prepares, binds and executes its own read with unchanged binds.
 * Callers pass trusted constant columns/table/order/limit; no request text is interpolated. */
export const legalItemOrderedSql = (
  columns: string,
  table: string,
  order: string,
  limit?: number,
) => `SELECT ${columns} FROM ${table} WHERE item_id=? ORDER BY ${order}${limit === undefined ? '' : ` LIMIT ${limit}`}`;
