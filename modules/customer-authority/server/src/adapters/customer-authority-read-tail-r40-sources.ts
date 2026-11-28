import 'server-only';

/** Rank 2 universal projection head shared by the Customer Authority r25,
 *  r28, r31, r34, r35 and r36 projection helpers (G4 read-tail r40). Callers
 *  pass their entire closed projection and keep their own source, predicate,
 *  prepared statement and binds, so every rendered statement stays byte-identical
 *  to the pre-dedupe literal. Text only: no execution, no writes, no locks.
 *  The r21-r39 helpers, named wrappers, native kinds and eligibility helpers
 *  stay untouched. */
export const selectProjection = (projection: string) => `SELECT ${projection}`;
