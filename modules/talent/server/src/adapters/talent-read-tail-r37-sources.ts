import "server-only";

/** Rank 2 universal projection head shared by the Talent r25 source-image,
 *  r32 source-row, scope actor, r26 evidence-page, executive metric and
 *  r23 scoped-materialized projection helpers (G4 read-tail r37). Callers pass
 *  their entire closed projection and keep their own source, predicate,
 *  prepared statement and binds, so every rendered statement stays
 *  byte-identical to the pre-dedupe literal. Text only: no execution, no
 *  writes, no locks. The r20-r36 families, named wrappers, locks and native
 *  read ports stay untouched. */
export const selectProjection = (projection: string) => `SELECT ${projection}`;
