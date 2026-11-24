import "server-only";
/** Byte-identical Legal recent-item list SQL shared by owner adapters (round 31).
 * Text only: every call site still prepares, binds and executes its own read with unchanged binds. */
/** Fixed five-column recent-item list; the caller supplies the projection, fact table and row limit. */
export const legalRecentItemListSql = (columns: string, table: string, limit: number) =>
  `SELECT ${columns} FROM ${table} WHERE item_id=? ORDER BY created_at DESC LIMIT ${limit}`;
