import 'server-only';

/** G4 / retire.organization.read-tail-r59 owner-local renderer.
 *
 * This export owns the single head formerly repeated at the two executive
 * metric projection call sites. Callers pass their verbatim body and label;
 * the rendered SQL, projection, bind order and arity stay byte-identical.
 * Pure read only: no writes, row/table locks, advisory statements, native
 * plans or cross-owner tails. r48-r58 helpers stay untouched. */

/** Count-by-label projection head. `body` is the verbatim FROM clause onward. */
export const executiveMetricCountSql = (label: string, body: string) =>
  `SELECT ${label} label,count(*)::text value FROM ${body}`;
