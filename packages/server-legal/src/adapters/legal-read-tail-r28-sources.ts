import "server-only";
/** Byte-identical Legal company-scoped point-read SQL shared by owner adapters (round 28).
 * Text only: every call site still prepares, binds and executes its own read with unchanged binds. */
/** Fixed-table company-scoped point read; the caller supplies the fixed legal fact table. */
export const legalByIdAndCompanySql = (table: string) =>
  `SELECT * FROM ${table} WHERE id=? AND company_id=?`;
