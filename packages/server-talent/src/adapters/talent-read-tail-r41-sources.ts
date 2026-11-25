import "server-only";
/** Owner-local subquery-source existence head family (round 41). The two
 *  record-source guard sites share one byte-identical head emitted here; the
 *  alias, joins and predicate stay in each caller so rendered SQL and binds
 *  remain byte-identical to the former inline copies. Distinct from r36's
 *  top-level `talentExistsSelect` (plain table source) and from the
 *  EXISTS-wrapped guard families. Text only: nothing is cached or executed. */
export const talentSourceSubqueryExists = (source: string, body: string) => `SELECT 1 FROM (${source}) ${body}`;
