import "server-only";
/** Byte-identical Legal item point-read SQL shared by owner adapters (round 27).
 * Text only: every call site still prepares, binds and executes its own read with unchanged binds. */
/** Fixed-table item point read; the caller supplies the fixed legal fact table. */
export const legalByIdAndItemSql = (table: string) =>
  `SELECT * FROM ${table} WHERE id=? AND item_id=?`;
