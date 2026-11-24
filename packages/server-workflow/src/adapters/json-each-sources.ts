/** Byte-identical generator family for erpx_json_each value reads: the caller
 * supplies only the expansion expression, emitted SQL is unchanged. Read-only. */
export const jsonEachSql = (expr: string): string => `SELECT value FROM erpx_json_each(${expr})`;
/** Byte-identical generator family for the approver-pool JSON-expansion shape:
 * the caller supplies only the pool column expression, emitted SQL is unchanged.
 * Read-only: this is a pure read fragment with no writes or row locks. */
export const jsonEachApproverPoolSql = (poolExpr: string): string =>
  jsonEachSql(`COALESCE(${poolExpr},'[]')`);
