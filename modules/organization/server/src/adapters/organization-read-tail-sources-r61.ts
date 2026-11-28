import 'server-only';

/** G4 / retire.organization.read-tail-r61 owner-local renderer.
 *
 * This export owns the single head formerly repeated at the two people
 * lifecycle position-package read call sites. Callers pass their verbatim
 * tail; the rendered SQL, projection, bind order and arity stay byte-identical.
 * Pure read only: no writes, row/table locks, advisory statements, native
 * plans or cross-owner tails. r48-r60 helpers stay untouched. */

/** Position-package projection head. `tail` is the verbatim remainder of the
 * query, including the owner projection fragment and every placeholder. */
export const positionPackageProjectionSql = (tail: string) =>
  `SELECT p.id,p.title,p.version${tail}`;
