/** r30: parameterized SELECT-head shared by the two reconciliation-runs
 * readiness readers. The head and trailing ORDER/LIMIT are byte-identical; only
 * the optional WHERE predicate varies. Text only: every call site still
 * prepares its own statement in its supplied database with its own binds, and
 * no result is cached here. Locks, advisory statements, native write
 * boundaries and every r1-r29 helper are left untouched. */

/** Final-reconciliation readiness: the optional WHERE is caller-supplied text
 * and the rendered statement is byte-identical to each original literal. */
export const reconciliationReadySql = (where: string) =>
  `SELECT ready FROM attachment_reconciliation_runs${where ? ` WHERE ${where}` : ''} ORDER BY created_at DESC LIMIT 1`;
