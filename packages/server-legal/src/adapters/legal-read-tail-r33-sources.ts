import "server-only";
/** State-constrained point reads shared by Legal owner adapters (round 33).
 * Text only: every call site still prepares, binds and executes its own read with unchanged binds.
 * Callers pass trusted constant identifiers/state literals; no request text is interpolated. */
export const legalStatePointSql = (
  columns: string,
  table: string,
  key2: 'company_id' | 'document_id' | 'rule_id',
  state: 'active' | 'approved',
) => `SELECT ${columns} FROM ${table} WHERE id=? AND ${key2}=? AND state='${state}'`;
