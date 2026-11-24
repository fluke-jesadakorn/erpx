import "server-only";
/** Byte-identical Legal single-key point-read SQL shared by owner adapters (round 32).
 * Text only: every call site still prepares, binds and executes its own read with unchanged binds. */
/** Fixed single-column-key point read; the caller supplies the projection and fact table. */
export const legalPointByIdSql = (columns: string, table: string) =>
  `SELECT ${columns} FROM ${table} WHERE id=?`;
