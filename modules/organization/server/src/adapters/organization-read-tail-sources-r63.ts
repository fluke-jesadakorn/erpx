import 'server-only';

/** G4 / retire.organization.read-tail-r63 owner-local renderer.
 *
 * This export owns the single node-identity projection head formerly repeated
 * at the workspace-choice node read and the executive main-evidence node read.
 * Callers pass their verbatim tail; the rendered SQL, projection, bind order and
 * arity stay byte-identical. Pure read only: no writes, row/table locks,
 * advisory statements, native plans or cross-owner tails. r50-r62 helpers stay
 * untouched. */

/** Node identity projection head. `tail` is the verbatim remainder of the
 * query, beginning with the next projected column. */
export const nodeIdentityHeadSql = (tail: string) =>
  `SELECT n.id,n.kind,n.slug,n.display_name,${tail}`;
