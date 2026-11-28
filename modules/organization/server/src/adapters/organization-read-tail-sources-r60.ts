import 'server-only';

/** G4 / retire.organization.read-tail-r60 owner-local renderer.
 *
 * This export owns the single head formerly repeated at the two member
 * level-history read call sites. Callers pass their verbatim tail; the
 * rendered SQL, projection, bind order and arity stay byte-identical.
 * Pure read only: no writes, row/table locks, advisory statements, native
 * plans or cross-owner tails. r48-r59 helpers stay untouched. */

/** Member level-history head. `tail` is the verbatim remainder of the query. */
export const levelHistorySql = (tail: string) =>
  `SELECT h.*,l.name level_name${tail}`;
