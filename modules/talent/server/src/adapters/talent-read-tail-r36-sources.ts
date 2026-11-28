import "server-only";
/** Owner-local standalone existence-read head family (round 36). The row source
 * and predicate are parameterized; the emitted read stays byte-identical to
 * the former per-adapter inline copies. Distinct from the EXISTS-wrapped guard
 * family: these are top-level reads, not assertion fragments. Text only:
 * nothing is cached or executed here. */
export const talentExistsSelect = (source: string, predicate: string) => `SELECT 1 FROM ${source} WHERE ${predicate}`;
