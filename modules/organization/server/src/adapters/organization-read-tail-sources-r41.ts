import 'server-only';

/** r41 additions: byte-identical relocation-snapshot version reads. Each call
 * renders exactly the literal it replaces; callers keep their own binds, scope
 * and statements. No r17-r40 helper, FOR UPDATE/FOR SHARE lock or spaced JOIN
 * constant is changed. Only owner-supplied relation and predicate vary (never
 * caller input). */

/** Company-scoped id/version snapshot parameterized by relation and predicate.
 * Family: the relocation-snapshot positions, assignments and GPO-rule reads.
 * Each call renders exactly the replaced literal; only the owner-supplied
 * relation and WHERE predicate vary. Text only: callers keep their own binds. */
export const relocationVersionSnapshot = (relation: string, predicate: string) =>
  `SELECT id,version FROM ${relation} WHERE company_id=? AND ${predicate} ORDER BY id`;
