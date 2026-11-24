import "server-only";
/** Byte-identical Legal ordered history list SQL shared by owner adapters (round 30).
 * Text only: every call site still prepares, binds and executes its own read with unchanged binds. */
/** Fixed-column ordered history list; the caller supplies the fixed legal fact table and key column. */
export const legalOrderedHistorySql = (table: string, key: string) =>
  `SELECT * FROM ${table} WHERE ${key}=? ORDER BY created_at,id`;
