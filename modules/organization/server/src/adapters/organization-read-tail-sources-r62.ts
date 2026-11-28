import 'server-only';

/** G4 / retire.organization.read-tail-r62 owner-local renderer.
 *
 * This export owns the single head formerly repeated at the commercial usage
 * node read and the executive context node read. Callers pass their verbatim
 * tail; the rendered SQL, projection, bind order and arity stay byte-identical.
 * Pure read only: no writes, row/table locks, advisory statements, native
 * plans or cross-owner tails. r48-r61 helpers stay untouched. */

/** Node kind/display-name projection head. `tail` is the verbatim remainder
 * of the query, beginning with the next projected column. */
export const nodeKindDisplayNameHeadSql = (tail: string) =>
  `SELECT id,kind,display_name,${tail}`;
