/** G4 / retire.projects.read-tail-r40 closed universal projection head.
 *
 * The remaining three named read wrappers — `projectPointReadHead` (r33),
 * `staffTaskPointHead` and `staffProjectByRefHead` (r34) — all begin with this
 * one projection-from head literal. Each wrapper keeps its own
 * closed projection/relation fragments, predicates, binds and result type;
 * row/table locks stay at the callers. No request SQL enters this helper and
 * no write or lock text is emitted. Cross-owner final-root projections have
 * their own quoted helper and are deliberately excluded from this fold.
 */
export const selectProjectionFrom = (columns: string, from: string) =>
  `SELECT ${columns} FROM ${from}`;
