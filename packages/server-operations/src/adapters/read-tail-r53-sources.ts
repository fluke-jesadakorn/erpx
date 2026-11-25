import 'server-only';
/** Canonical owner-local heads for the Operations task read tail. Both are the
 *  exact retired byte prefixes; each caller appends its own unchanged tail so
 *  rendered SQL, binds/order and locks stay identical. */
export const OPERATIONS_STAFF_TASK_HEAD = (alias: string) => `SELECT 1 FROM staff_tasks ${alias} `;
export const OPERATIONS_TASK_LIST_HEAD = 'SELECT t.id,t.title,t.state,t.';
