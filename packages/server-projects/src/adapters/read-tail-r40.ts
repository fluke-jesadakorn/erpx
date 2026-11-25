/** G4 / retire.projects.read-tail-r40 closed universal projection head.
 *
 * The remaining three named read wrappers — `projectPointReadHead` (r33),
 * `staffTaskPointHead` and `staffProjectByRefHead` (r34) — all begin with this
 * one projection-from head literal. Each wrapper keeps its own
 * closed projection/relation fragments, predicates, binds and result type;
 * row/table locks stay at the callers. No request SQL enters this helper and
 * no write or lock text is emitted. The cross-owner final-root projection
 * family (r44) now also delegates its closed quoted sources here, so one
 * universal head now owns every projection/from rendering in the owner.
 */
export const selectProjectionFrom = (columns: string, from: string) =>
  `SELECT ${columns} FROM ${from}`;
