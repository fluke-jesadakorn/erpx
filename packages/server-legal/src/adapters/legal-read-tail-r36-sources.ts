import "server-only";
/** Byte-identical Legal arbitrary-key point-read SQL shared by owner adapters (round 36).
 * Text only: every call site still prepares, binds and executes its own read with unchanged binds.
 * Callers pass trusted constant columns/table/key; no request text is interpolated. */
export const legalPointByKeySql = (
  columns: string,
  table: string,
  key: 'document_id' | 'id',
) => `SELECT ${columns} FROM ${table} WHERE ${key}=?`;
