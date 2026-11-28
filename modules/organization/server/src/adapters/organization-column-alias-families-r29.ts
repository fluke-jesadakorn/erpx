import 'server-only';
/** r29 additions: relation/predicate-parameterized owner read SQL. These families
 * are new; every r17-r28 helper and its call sites are left untouched. Every
 * call renders byte-identical SQL to the literal it replaces; only owner-supplied
 * relations, predicates, columns and tables vary (never caller input). Text only:
 * callers keep their own binds and supplied statements. */

/** Company-bound max-version subquery parameterized by relation and id column.
 * Family: the level-leave and position-pay catalog/leave snapshot reads. */
export const maxVersionSubquery = (table: string, column: string) =>
  `(SELECT max(x.version) FROM ${table} x WHERE x.company_id=d.company_id AND x.${column}=d.${column})`;

/** Company-scoped impact count parameterized by relation and predicate.
 * Family: the five node-impact count read clauses. */
export const impactCount = (from: string, predicate: string) =>
  `(SELECT count(*) FROM ${from} WHERE company_id=? AND ${predicate})`;
