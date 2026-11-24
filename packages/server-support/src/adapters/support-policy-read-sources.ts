import "server-only";
/** One parameterized family for the two fixed Support SLA policy reads. The
 * table stays owner-local; only the projected columns, the WHERE predicate and
 * the trailing ORDER vary. Each call site renders byte-identically and the
 * native Support policy readers remain the authority. */
export const supportPolicySelect = (columns: string, where: string, order = '') =>
    `SELECT ${columns} FROM support_sla_policies WHERE ${where}${order}`;
