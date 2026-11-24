import "server-only";
/** Byte-identical Legal id+key point-read SQL shared by owner adapters (round 34).
 * Text only: every call site still prepares, binds and executes its own read with unchanged binds.
 * Callers pass trusted constant columns/table/key; no request text is interpolated. */
export const legalPointByIdAndKeySql = (
  columns: string,
  table: string,
  key2: 'user_id' | 'company_id' | 'item_id',
) => `SELECT ${columns} FROM ${table} WHERE id=? AND ${key2}=?`;
