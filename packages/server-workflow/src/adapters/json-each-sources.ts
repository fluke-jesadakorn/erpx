/** Byte-identical generator family for erpx_json_each value reads: the caller
 * supplies only the expansion expression, emitted SQL is unchanged. Read-only. */
export const jsonEachSql = (expr: string): string => `SELECT value FROM erpx_json_each(${expr})`;
/** Byte-identical generator family for the approver-pool JSON-expansion shape:
 * the caller supplies only the pool column expression, emitted SQL is unchanged.
 * Read-only: this is a pure read fragment with no writes or row locks. */
export const jsonEachApproverPoolSql = (poolExpr: string): string =>
  jsonEachSql(`COALESCE(${poolExpr},'[]')`);
/** Byte-identical approver-union projection family: the closed variant selects
 * only the alias/pool expression shape, emitted SQL is unchanged. Read-only. */
export const approverUnionProjectionSql = (variant: 'step' | 'plain'): string =>
  `SELECT ${variant === 'step' ? 'ast.approver_user_id user_id' : 'approver_user_id'} UNION ${jsonEachApproverPoolSql(variant === 'step' ? 'ast.approver_pool_json' : 'approver_pool_json')}`;
